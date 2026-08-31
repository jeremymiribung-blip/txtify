use crate::core::error::TxtifyError;
use crate::core::traits::FormatDetector;
use crate::core::types::InputFormat;

/// Default format detector using extension and magic bytes.
pub struct DefaultDetector;

impl FormatDetector for DefaultDetector {
    fn detect(&self, path: &std::path::Path) -> Result<InputFormat, TxtifyError> {
        // Try magic bytes first, especially %PDF, to handle files with wrong extension
        // We read a small header (up to 8KB) to avoid loading large files fully
        if let Ok(bytes) = read_magic_bytes(path) {
            if bytes.starts_with(b"%PDF") {
                return Ok(InputFormat::Pdf);
            }
            if let Some(fmt) = InputFormat::from_magic_bytes(&bytes) {
                return Ok(fmt);
            }
        }

        // Try extension
        if let Some(fmt) = InputFormat::from_extension(path) {
            return Ok(fmt);
        }

        // Fall back to full magic bytes check if header read succeeded but extension unknown
        // (already tried magic, but we try again with full file if header was truncated)
        // If header read failed (file not found), propagate Io if extension also unknown?
        // To preserve previous behavior, attempt full read if not yet succeeded
        match std::fs::read(path) {
            Ok(bytes) => {
                if let Some(fmt) = InputFormat::from_magic_bytes(&bytes) {
                    return Ok(fmt);
                }
            }
            Err(e) => {
                // If extension already failed, return Io error for missing file
                // But if path had no extension and file missing, original code returned Io via map_err
                // We mimic that: if extension is None, return Io
                if InputFormat::from_extension(path).is_none() {
                    return Err(TxtifyError::Io(e));
                }
                // else extension would have returned earlier, so unreachable
            }
        }

        Err(TxtifyError::UnsupportedFormat(format!(
            "cannot detect format for {}",
            path.display()
        )))
    }
}

fn read_magic_bytes(path: &std::path::Path) -> Result<Vec<u8>, std::io::Error> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 8192];
    let n = file.read(&mut buf)?;
    buf.truncate(n);
    Ok(buf)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::core::types::ImageFormat;
    use std::path::PathBuf;

    // Property test for detector: extension case-insensitivity and magic bytes determinism
    #[cfg(test)]
    mod property {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn extension_case_insensitive(base in "[a-z]{1,8}", ext in "(pdf|docx|xlsx|pptx|html|png|jpg|tiff|txt|md|csv)") {
                let ext_lower = ext.to_ascii_lowercase();
                let ext_upper = ext.to_ascii_uppercase();
                let mixed: String = ext.chars().enumerate().map(|(i,c)| if i%2==0 {c.to_ascii_uppercase()} else {c.to_ascii_lowercase()}).collect();
                let lower = format!("{base}.{ext_lower}");
                let upper = format!("{base}.{ext_upper}");
                let mixed_path = format!("{base}.{mixed}");
                let p_lower = PathBuf::from(&lower);
                let p_upper = PathBuf::from(&upper);
                let p_mixed = PathBuf::from(&mixed_path);
                let fmt_lower = InputFormat::from_extension(&p_lower);
                let fmt_upper = InputFormat::from_extension(&p_upper);
                let fmt_mixed = InputFormat::from_extension(&p_mixed);
                prop_assert_eq!(fmt_lower.clone(), fmt_upper);
                prop_assert_eq!(fmt_lower, fmt_mixed);
            }

            #[test]
            fn magic_bytes_deterministic(bytes in proptest::collection::vec(0u8..255u8, 0..20)) {
                // from_magic_bytes should be deterministic and never panic
                let a = InputFormat::from_magic_bytes(&bytes);
                let b = InputFormat::from_magic_bytes(&bytes);
                prop_assert_eq!(a, b);
            }

            #[test]
            fn detector_deterministic(path in "[a-z]{1,5}\\.(pdf|txt|md)") {
                let p = PathBuf::from(path);
                let det = DefaultDetector;
                // detect may fail if file missing, but should be deterministic
                let r1 = det.detect(&p).map(|f| format!("{f:?}")).unwrap_or_else(|e| e.to_string());
                let r2 = det.detect(&p).map(|f| format!("{f:?}")).unwrap_or_else(|e| e.to_string());
                prop_assert_eq!(r1, r2);
            }
        }
    }

    #[test]
    fn detect_by_extension() {
        let det = DefaultDetector;
        let p = PathBuf::from("report.pdf");
        assert_eq!(det.detect(&p).unwrap(), InputFormat::Pdf);
    }

    #[test]
    fn detect_unknown_ext_falls_back_to_magic() {
        let det = DefaultDetector;
        // No extension => error (file doesn't exist for magic bytes either)
        let p = PathBuf::from("noext");
        assert!(det.detect(&p).is_err());
    }

    #[test]
    fn detect_missing_file() {
        let det = DefaultDetector;
        let p = PathBuf::from("nonexistent.xyz");
        assert!(det.detect(&p).is_err());
    }

    #[test]
    fn extension_based_detection_covers_all_variants() {
        let cases: Vec<(&str, InputFormat)> = vec![
            ("a.pdf", InputFormat::Pdf),
            ("b.docx", InputFormat::Docx),
            ("c.xlsx", InputFormat::Xlsx),
            ("d.pptx", InputFormat::Pptx),
            ("e.html", InputFormat::Html),
            ("f.png", InputFormat::Image(ImageFormat::Png)),
            ("g.jpg", InputFormat::Image(ImageFormat::Jpeg)),
            ("h.tiff", InputFormat::Image(ImageFormat::Tiff)),
            ("i.txt", InputFormat::Txt),
            ("j.md", InputFormat::Md),
            ("k.csv", InputFormat::Csv),
        ];

        for (name, expected) in cases {
            let p = PathBuf::from(name);
            assert_eq!(
                InputFormat::from_extension(&p),
                Some(expected),
                "failed for {name}"
            );
        }
    }

    #[test]
    fn detect_pdf_via_magic_bytes() {
        let det = DefaultDetector;
        let tmp = std::env::temp_dir().join("txtify_test_pdf_magic_detect");
        std::fs::write(&tmp, b"%PDF-1.4 test content").unwrap();
        let fmt = det.detect(&tmp).unwrap();
        assert_eq!(fmt, InputFormat::Pdf);
        let _ = std::fs::remove_file(&tmp);

        // Misnamed extension should still be detected as Pdf via magic priority
        let tmp2 = std::env::temp_dir().join("txtify_test_pdf_magic_misnamed.docx");
        std::fs::write(&tmp2, b"%PDF-1.4 misnamed").unwrap();
        let fmt2 = det.detect(&tmp2).unwrap();
        assert_eq!(fmt2, InputFormat::Pdf);
        let _ = std::fs::remove_file(&tmp2);
    }
}
