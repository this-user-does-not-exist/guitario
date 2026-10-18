//! The sampled sound bank: which instrument a track plays, what the bank holds, and installing
//! the recommended open libraries into it (`guitario_playback::install`).

use crate::commands::{CommandSpec, cmd, mixer_track};
use crate::{EngineError, Session};
use guitario_playback::Role;
use serde_json::{Value, json};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd(
            "track.setBank",
            "Sound Bank",
            "Sound",
            None,
            "bank (a role: drums, bass, guitar_clean, guitar_distorted; or an .sfz file, absolute or relative to the bank's folder; null: the one the instrument picks), \
             track? (index; default the cursor's)",
            true,
            set_bank,
        ),
        cmd(
            "sound.bank",
            "Sound Bank Contents",
            "Sound",
            None,
            "dir? (the bank's folder; default $GUITARIO_SOUNDS or the per-user one): the folder, what plays each role, the recommended libraries and whether each is installed, and the credits their licences ask for",
            false,
            bank_info,
        ),
        cmd(
            "sound.installLibraries",
            "Install Sound Libraries",
            "Sound",
            None,
            "ids (library ids from sound.bank, or \"all\"), dir? (default as sound.bank): downloads (with curl, resuming) and unpacks each, then points bank.json at them; hundreds of MB each",
            false,
            install_libraries,
        ),
    ]
}

fn set_bank(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let bank = match params.get("bank") {
        None => return Err(EngineError::BadParam("`bank` is required (a role, an .sfz file, or null)".into())),
        Some(Value::Null) => None,
        Some(Value::String(b)) => {
            let b = b.trim();
            let is_sfz = std::path::Path::new(b).extension().is_some_and(|e| e.eq_ignore_ascii_case("sfz"));
            if Role::from_key(b).is_none() && !is_sfz {
                let roles: Vec<&str> = Role::ALL.iter().map(|r| r.key()).collect();
                return Err(EngineError::BadParam(format!("`bank` must be one of {} or an .sfz file, not `{b}`", roles.join(", "))));
            }
            if b.len() > guitario_score::MAX_BANK_NAME || b.contains('\0') {
                return Err(EngineError::BadParam("`bank` is too long".into()));
            }
            Some(b.to_string())
        }
        Some(_) => return Err(EngineError::BadParam("`bank` must be a string or null".into())),
    };
    let t = mixer_track(s, params)?;
    t.bank = bank;
    Ok(json!({"bank": t.bank}))
}

#[cfg(not(target_arch = "wasm32"))]
fn bank_dir(params: &Value) -> Result<std::path::PathBuf, EngineError> {
    match crate::params::opt_string(params, "dir") {
        Some(d) if !d.trim().is_empty() => Ok(std::path::PathBuf::from(d)),
        Some(_) => Err(EngineError::BadParam("`dir` cannot be empty".into())),
        None => {
            guitario_playback::SoundBank::default_dir().ok_or_else(|| EngineError::invalid("no folder for the sound bank: set GUITARIO_SOUNDS or pass `dir`"))
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn bank_info(_s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    use guitario_playback::install::LIBRARIES;
    use guitario_playback::{Articulation, SoundBank};
    let dir = bank_dir(params)?;
    let bank = SoundBank::open(&dir).ok();
    let roles: Vec<Value> = Role::ALL
        .iter()
        .map(|r| {
            let program = bank.as_ref().and_then(|b| b.program(*r, Articulation::Sustain)).map(|p| p.path.display().to_string());
            let library = LIBRARIES.iter().find(|l| l.role == *r && program.as_deref().is_some_and(|p| std::path::Path::new(p).starts_with(l.dir(&dir))));
            json!({"role": r.key(), "program": program, "library": library.map(|l| l.id)})
        })
        .collect();
    let libraries: Vec<Value> = LIBRARIES
        .iter()
        .map(|l| {
            json!({"id": l.id, "role": l.role.key(), "title": l.title, "license": l.license, "credit": l.credit, "mb": l.mb, "installed": l.installed(&dir)})
        })
        .collect();
    let credits: Vec<String> =
        LIBRARIES.iter().filter(|l| l.needs_credit() && l.installed(&dir)).map(|l| format!("{} ({}): {}", l.title, l.license, l.credit)).collect();
    Ok(json!({"dir": dir.display().to_string(), "bankFile": bank.is_some(), "roles": roles, "libraries": libraries, "credits": credits}))
}

#[cfg(target_arch = "wasm32")]
fn bank_info(_s: &mut Session, _params: &Value) -> Result<Value, EngineError> {
    Err(EngineError::invalid("sound banks are not supported in the browser build"))
}

#[cfg(not(target_arch = "wasm32"))]
fn install_libraries(_s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    use guitario_playback::install::{LIBRARIES, Library, install, write_bank};
    let wanted: Vec<&'static Library> = match params.get("ids") {
        Some(Value::String(all)) if all == "all" => LIBRARIES.iter().collect(),
        Some(Value::Array(ids)) if !ids.is_empty() => ids
            .iter()
            .map(|id| id.as_str().and_then(Library::by_id).ok_or_else(|| EngineError::BadParam(format!("`ids`: no library {id}; see sound.bank"))))
            .collect::<Result<_, _>>()?,
        _ => return Err(EngineError::BadParam("`ids` must list library ids (see sound.bank) or be \"all\"".into())),
    };
    let dir = bank_dir(params)?;
    std::fs::create_dir_all(&dir)?;
    let never = std::sync::atomic::AtomicBool::new(false);
    let (mut installed, mut failed) = (Vec::new(), Vec::new());
    for lib in wanted {
        match install(lib, &dir, &mut |_| {}, &never) {
            Ok(()) => installed.push(lib.id),
            Err(e) => failed.push(json!({"id": lib.id, "error": e})),
        }
    }
    let roles: Vec<&str> = write_bank(&dir).map_err(EngineError::invalid)?.iter().map(|r| r.key()).collect();
    Ok(json!({"dir": dir.display().to_string(), "installed": installed, "failed": failed, "roles": roles}))
}

#[cfg(target_arch = "wasm32")]
fn install_libraries(_s: &mut Session, _params: &Value) -> Result<Value, EngineError> {
    Err(EngineError::invalid("sound banks are not supported in the browser build"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_track_names_its_instrument_and_it_is_saved() {
        let mut s = Session::new();
        assert_eq!(s.run("track.setBank", &json!({"bank": "guitar_clean"})).unwrap()["bank"], "guitar_clean");
        assert_eq!(s.score().tracks[0].bank.as_deref(), Some("guitar_clean"));
        s.run("track.setBank", &json!({"bank": "My Library/lead.SFZ"})).unwrap();
        let text = guitario_format::to_bytes(s.score()).unwrap();
        assert_eq!(guitario_format::from_bytes(&text).unwrap().tracks[0].bank.as_deref(), Some("My Library/lead.SFZ"));
        s.run("track.setBank", &json!({"bank": null})).unwrap();
        assert_eq!(s.score().tracks[0].bank, None);
        s.run("edit.undo", &Value::Null).unwrap();
        assert_eq!(s.score().tracks[0].bank.as_deref(), Some("My Library/lead.SFZ"), "one undo step");
        for bad in [json!({}), json!({"bank": "piano"}), json!({"bank": 5}), json!({"bank": "x.wav"}), json!({"bank": format!("{}.sfz", "a".repeat(5000))})] {
            assert!(s.run("track.setBank", &bad).is_err(), "{bad}");
        }
        // Older files have no such field and still load.
        let mut v: Value = serde_json::from_slice(&guitario_format::to_bytes(&guitario_score::Score::new()).unwrap()).unwrap();
        assert!(v["score"]["tracks"][0].get("bank").is_none(), "left out while unset");
        v["score"]["tracks"][0]["bank"] = Value::Null;
        assert!(guitario_format::from_bytes(&serde_json::to_vec(&v).unwrap()).is_ok());
    }

    #[test]
    fn the_bank_reports_its_roles_libraries_and_credits_and_installs_what_is_there() {
        let dir = std::env::temp_dir().join(format!("gt-engine-bank-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // A library already in place: installing it again downloads nothing.
        let kit = dir.join("muldjordkit/Kit/Stereo");
        std::fs::create_dir_all(&kit).unwrap();
        std::fs::write(kit.join("kit.sfz"), "<region> sample=a.wav key=36").unwrap();
        std::fs::write(dir.join("muldjordkit/.complete"), "").unwrap();
        let mut s = Session::new();
        let d = dir.display().to_string();
        let info = s.run("sound.bank", &json!({"dir": d})).unwrap();
        assert_eq!(info["bankFile"], false);
        assert_eq!(info["libraries"].as_array().unwrap().len(), 4);
        assert!(info["credits"][0].as_str().unwrap().contains("Muldjord"), "{info}");
        let done = s.run("sound.installLibraries", &json!({"ids": ["muldjordkit"], "dir": d})).unwrap();
        assert_eq!(done["installed"][0], "muldjordkit");
        assert_eq!(done["roles"], json!(["drums"]));
        let info = s.run("sound.bank", &json!({"dir": d})).unwrap();
        let drums = info["roles"].as_array().unwrap().iter().find(|r| r["role"] == "drums").unwrap().clone();
        assert_eq!(drums["library"], "muldjordkit");
        assert!(drums["program"].as_str().unwrap().ends_with("kit.sfz"));
        // Nothing is downloaded without naming what.
        for bad in [Value::Null, json!({}), json!({"ids": []}), json!({"ids": ["nope"], "dir": d}), json!({"ids": "some"})] {
            assert!(s.run("sound.installLibraries", &bad).is_err(), "{bad}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
