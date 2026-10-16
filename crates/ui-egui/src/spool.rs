//! The system's print queues, through craft-print (CUPS on macOS and Linux, the native print API
//! on Windows). Register [`SystemSpooler`] with [`guitario_engine::Session::set_spooler`] to enable
//! `file.print` and `file.printers`.

use craft_print::{Duplex, Job, Scaling};
use guitario_engine::{DuplexMode, PrintJob, PrinterInfo, Spooler};

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemSpooler;

/// The craft-print job for ours. Pages are A4, so a printer loaded with another paper gets them
/// shrunk to fit instead of refused.
fn job_of(job: &PrintJob) -> Job {
    Job {
        printer: job.printer.clone(),
        copies: job.copies,
        collate: job.collate,
        duplex: match job.duplex {
            DuplexMode::Default => None,
            DuplexMode::Off => Some(Duplex::Off),
            DuplexMode::LongEdge => Some(Duplex::LongEdge),
            DuplexMode::ShortEdge => Some(Duplex::ShortEdge),
        },
        scaling: Scaling::ShrinkToFit,
        grayscale: job.grayscale,
        title: job.title.clone(),
        options: Vec::new(),
    }
}

impl Spooler for SystemSpooler {
    fn printers(&self) -> Vec<PrinterInfo> {
        craft_print::printers().into_iter().map(|p| PrinterInfo { name: p.name, default: p.default }).collect()
    }

    fn submit(&self, pdf: &[u8], job: &PrintJob) -> Result<String, String> {
        craft_print::submit(pdf, &job_of(job)).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn our_job_maps_onto_the_spooler_job() {
        let ours = PrintJob { printer: Some("Office".into()), copies: 4, collate: false, duplex: DuplexMode::ShortEdge, grayscale: true, title: "Riff".into() };
        let theirs = job_of(&ours);
        assert_eq!(
            (theirs.printer.as_deref(), theirs.copies, theirs.collate, theirs.duplex, theirs.grayscale),
            (Some("Office"), 4, false, Some(Duplex::ShortEdge), true)
        );
        assert_eq!(theirs.title, "Riff");
        let default = job_of(&PrintJob { duplex: DuplexMode::Default, printer: None, ..ours.clone() });
        assert_eq!((default.duplex, default.printer), (None, None));
        assert_eq!(job_of(&PrintJob { duplex: DuplexMode::Off, ..ours }).duplex, Some(Duplex::Off));
    }

    #[test]
    fn the_command_line_for_a_job_names_the_printer_and_copies() {
        let job =
            job_of(&PrintJob { printer: Some("Office".into()), copies: 2, collate: true, duplex: DuplexMode::LongEdge, grayscale: false, title: "T".into() });
        let args = craft_print::lp_args(&job).join(" ");
        assert!(args.contains("Office") && args.contains('2'), "{args}");
    }
}
