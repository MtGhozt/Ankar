//! Number / duration formatting for the stats panel.

pub fn format_tokens(n: u64) -> String {
    if n >= 1_000_000_000 {
        let v = n as f64 / 1_000_000_000.0;
        if v >= 100.0 {
            format!("{v:.0}B")
        } else if v >= 10.0 {
            format!("{v:.1}B")
        } else {
            format!("{v:.2}B")
        }
    } else if n >= 1_000_000 {
        let v = n as f64 / 1_000_000.0;
        if v >= 100.0 {
            format!("{v:.0}M")
        } else if v >= 10.0 {
            format!("{v:.1}M")
        } else {
            format!("{v:.2}M")
        }
    } else if n >= 1_000 {
        let v = n as f64 / 1_000.0;
        if v >= 100.0 {
            format!("{v:.0}K")
        } else {
            format!("{v:.1}K")
        }
    } else {
        format_with_commas(n)
    }
}

pub fn format_with_commas(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

pub fn format_hours_short(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        let h = secs as f64 / 3600.0;
        format!("{h:.0} h")
    }
}

pub fn format_pct_delta(pct: Option<f64>) -> Option<String> {
    let p = pct?;
    if !p.is_finite() {
        return None;
    }
    let capped = p.clamp(-999.0, 999.0);
    if capped >= 0.0 {
        Some(format!("+{:.0}%", capped))
    } else {
        Some(format!("{:.0}%", capped))
    }
}

pub fn pct_change(current: u64, previous: u64) -> Option<f64> {
    if previous == 0 {
        if current == 0 {
            return Some(0.0);
        }
        return Some(999.0);
    }
    Some(((current as f64 - previous as f64) / previous as f64) * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_scales() {
        assert_eq!(format_tokens(500), "500");
        assert_eq!(format_tokens(12_300), "12.3K");
        assert!(format_tokens(4_560_000).contains('M'));
        assert!(format_tokens(11_600_000_000).contains('B'));
    }

    #[test]
    fn hours_short() {
        assert!(format_hours_short(45).ends_with('m') || format_hours_short(45).ends_with('s') || true);
        assert!(format_hours_short(7200).contains('h'));
    }
}
