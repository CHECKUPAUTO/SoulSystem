//! kmsg line parsing — pure function, no I/O.
//!
//! Extracted from `guardian/src/kmsg_reader.rs` so benches and other crates
//! can reuse it. The reader actor still owns the `/dev/kmsg` I/O loop; this
//! module owns only the parse decision.

/// Parse one kmsg line. Returns the XID code on match.
///
/// Matches:
/// * `NVRM: Xid (PCI:0000:03:00): 43, pid=1234, ...`
/// * `NVRM: Xid (PCI:0000:03:00.0): 79`
/// * `<6>[12345.678] NVRM: Xid (PCI:...): 44, ...`  (timestamp prefix)
/// * `6,12345,678901234,-;NVRM: Xid (PCI:...): 119, ...`  (raw kmsg format)
///
/// Lines that don't contain `NVRM: Xid (PCI:...)` are rejected silently
/// (kmsg is very chatty — we drop everything that's not ours).
pub fn parse_xid_line(line: &str) -> Option<u32> {
    let header = line.find("NVRM: Xid")?;
    let rest = &line[header..];
    let pci_open = rest.find("(PCI:")?;
    let close = rest[pci_open..].find("):")?;
    let after = &rest[pci_open + close + 2..]; // skip `):` itself
    let after = after.trim_start();
    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u32>().ok()
}

#[cfg(test)]
mod tests {
    use super::parse_xid_line;

    #[test]
    fn parses_standard_xid_line() {
        assert_eq!(parse_xid_line("NVRM: Xid (PCI:0000:03:00): 43, pid=1234, Ch 00000008"),
                   Some(43));
    }

    #[test]
    fn parses_xid_with_timestamp_prefix() {
        assert_eq!(parse_xid_line("<6>[12345.678] NVRM: Xid (PCI:0000:03:00): 44, Something"),
                   Some(44));
    }

    #[test]
    fn parses_xid_with_pci_function_suffix() {
        assert_eq!(parse_xid_line("NVRM: Xid (PCI:0000:03:00.0): 79"), Some(79));
    }

    #[test]
    fn parses_kmsg_raw_format() {
        assert_eq!(
            parse_xid_line("6,12345,678901234,-;NVRM: Xid (PCI:0000:03:00): 119, GSP timeout"),
            Some(119)
        );
    }

    #[test]
    fn ignores_non_nvrm_lines() {
        for line in [
            "kernel: usb 1-1: new high-speed device",
            "",
            "random text with NVRM but no Xid header",
            "NVRM: some other message without Xid",
        ] {
            assert_eq!(parse_xid_line(line), None, "should not parse: {line}");
        }
    }

    #[test]
    fn ignores_malformed_xid_lines() {
        for line in [
            "NVRM: Xid (PCI:0000:03:00)",
            "NVRM: Xid (PCI:0000:03:00): not-a-number",
            "NVRM: Xid PCI:0000:03:00: 43",
        ] {
            assert_eq!(parse_xid_line(line), None, "should not parse: {line}");
        }
    }

    #[test]
    fn extracts_only_leading_digits() {
        assert_eq!(parse_xid_line("NVRM: Xid (PCI:0000:03:00): 43abc, pid=1234"), Some(43));
    }

    #[test]
    fn handles_large_xid_codes() {
        assert_eq!(parse_xid_line("NVRM: Xid (PCI:0000:03:00): 2147, new high code"),
                   Some(2147));
    }
}
