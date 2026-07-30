//! Pure tool-argument mapping — hermes' generate_song semantics ported
//! faithfully (pct sliders, instrumental auto-detect, unhinged seed wrap,
//! advisory limit warnings). All testable without a network.

use serde_json::Value;
use sonus_core::GenerateParams;

/// (style, lyrics, title) advisory character limits per model —
/// hermes' MODEL_LIMITS table verbatim.
const MODEL_LIMITS: &[(&str, usize, usize, usize)] = &[
    ("V4", 200, 3000, 80),
    ("V4_5", 1000, 5000, 100),
    ("V4_5PLUS", 1000, 5000, 100),
    ("V4_5ALL", 1000, 5000, 80),
    ("V5", 1000, 5000, 100),
    ("V5_5", 1000, 5000, 100),
];
const LYRICS_STABILITY_TARGET: usize = 4000;

pub fn str_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// hermes: weirdness_pct/style_pct default 50.0, clamped into 0.0–1.0 —
/// the sliders are ALWAYS on the wire.
fn pct_arg(args: &Value, key: &str) -> f64 {
    (args.get(key).and_then(Value::as_f64).unwrap_or(50.0) / 100.0).clamp(0.0, 1.0)
}

/// hermes' auto-detect: no lyrics → instrumental; else instrumental when
/// under 40% of chars are letters/whitespace (symbols/kaomoji/binary count
/// as "not words").
pub fn auto_instrumental(lyrics: &str) -> bool {
    if lyrics.is_empty() {
        return true;
    }
    let total = lyrics.chars().count();
    let word_chars = lyrics
        .chars()
        .filter(|c| c.is_alphabetic() || c.is_whitespace())
        .count();
    (word_chars as f64 / total.max(1) as f64) < 0.4
}

/// hermes' unhinged-seed rule: wrap unless already `[[[`-wrapped, then ride
/// the lyrics field (never a separate wire field).
pub fn effective_lyrics(lyrics: &str, seed: &str) -> String {
    let seed = seed.trim();
    if seed.is_empty() {
        return lyrics.to_string();
    }
    let wrapped = if seed.starts_with("[[[") {
        seed.to_string()
    } else {
        format!("[[[“””{seed}”””]]]")
    };
    if lyrics.trim().is_empty() {
        wrapped
    } else {
        format!("{lyrics}\n\n{wrapped}")
    }
}

/// Advisory warnings (hermes' validate_limits): nothing blocks the request —
/// upstream truncates silently, so the agent gets told beforehand.
pub fn limit_warnings(
    model: &str,
    styles: &str,
    lyrics: &str,
    title: Option<&str>,
    instrumental: bool,
) -> Vec<String> {
    let mut w = Vec::new();
    let (s_lim, l_lim, t_lim) = match MODEL_LIMITS.iter().find(|(m, ..)| *m == model) {
        Some((_, s, l, t)) => (*s, *l, *t),
        None => {
            w.push(format!(
                "unknown model '{model}'; defaulting limits to V5 (1000/5000/100)"
            ));
            (1000, 5000, 100)
        }
    };
    let s_len = styles.chars().count();
    if s_len > s_lim {
        w.push(format!(
            "style is {s_len} chars (limit {s_lim} for {model}) — will be silently truncated by Suno"
        ));
    }
    let l_len = lyrics.chars().count();
    if l_len > l_lim {
        w.push(format!(
            "lyrics are {l_len} chars (limit {l_lim} for {model}) — will be silently truncated by Suno"
        ));
    } else if l_len > LYRICS_STABILITY_TARGET {
        w.push(format!(
            "lyrics are {l_len} chars, over the ~{LYRICS_STABILITY_TARGET} stability target — Suno's parser may get sloppy"
        ));
    }
    if let Some(t) = title {
        let t_len = t.chars().count();
        if t_len > t_lim {
            w.push(format!(
                "title is {t_len} chars (limit {t_lim} for {model}) — will be silently truncated by Suno"
            ));
        }
    }
    if !instrumental && lyrics.trim().is_empty() {
        w.push(
            "no lyrics on a vocal song — Suno will improvise; pass instrumental=true if that's wrong"
                .to_string(),
        );
    }
    w
}

/// generate_song args → (wire params, advisory warnings). Errors are
/// human-facing strings the tool returns as `{"error": …}` (hermes shape).
pub fn generate_params(args: &Value) -> Result<(GenerateParams, Vec<String>), String> {
    let styles = str_arg(args, "styles").ok_or(
        "styles is required — comma-separated descriptors, e.g. \
         \"dream pop, breathy female vocals, 80BPM\"",
    )?;
    let lyrics_raw = str_arg(args, "lyrics").unwrap_or_default();
    let seed = str_arg(args, "unhinged_seed").unwrap_or_default();
    let lyrics = effective_lyrics(&lyrics_raw, &seed);
    // hermes detects on the raw lyrics arg, before the seed rides along
    let instrumental = match args.get("instrumental").and_then(Value::as_bool) {
        Some(b) => b,
        None => auto_instrumental(&lyrics_raw),
    };
    let model = str_arg(args, "model").unwrap_or_else(|| "V5".to_string());
    let title = str_arg(args, "title");
    let warnings = limit_warnings(&model, &styles, &lyrics, title.as_deref(), instrumental);
    let params = GenerateParams {
        custom_mode: true, // the tool surface is field-driven = custom mode
        instrumental,
        model,
        style: Some(styles),
        title,
        prompt: (!lyrics.is_empty()).then_some(lyrics),
        negative_tags: str_arg(args, "exclude_styles"),
        weirdness: Some(pct_arg(args, "weirdness_pct")),
        style_weight: Some(pct_arg(args, "style_pct")),
        vocal_gender: str_arg(args, "vocal_gender"),
        callback_url: str_arg(args, "callback_url"),
    };
    Ok((params, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn styles_is_required() {
        assert!(generate_params(&json!({})).is_err());
        assert!(generate_params(&json!({"styles": "  "})).is_err());
    }

    #[test]
    fn sliders_default_to_half_and_clamp() {
        let (p, _) = generate_params(&json!({"styles": "lofi"})).unwrap();
        assert_eq!(p.weirdness, Some(0.5));
        assert_eq!(p.style_weight, Some(0.5));
        let (p, _) = generate_params(&json!({
            "styles": "lofi", "weirdness_pct": 250.0, "style_pct": -10.0
        }))
        .unwrap();
        assert_eq!(p.weirdness, Some(1.0));
        assert_eq!(p.style_weight, Some(0.0));
    }

    #[test]
    fn instrumental_auto_detect_matches_hermes() {
        assert!(auto_instrumental(""));
        assert!(!auto_instrumental("la la la real words here"));
        // symbols/binary: barely any letters → instrumental
        assert!(auto_instrumental("010101 110011 010110 ♥♦♣♠"));
        // explicit arg always wins
        let (p, _) =
            generate_params(&json!({"styles": "x", "lyrics": "hello world", "instrumental": true}))
                .unwrap();
        assert!(p.instrumental);
    }

    #[test]
    fn unhinged_seed_wraps_and_rides_the_lyrics() {
        assert_eq!(effective_lyrics("", ""), "");
        assert_eq!(effective_lyrics("", "chaos"), "[[[“””chaos”””]]]");
        assert_eq!(
            effective_lyrics("[Verse]\nhi", "chaos"),
            "[Verse]\nhi\n\n[[[“””chaos”””]]]"
        );
        // pre-wrapped seeds pass through untouched
        assert_eq!(effective_lyrics("", "[[[already]]]"), "[[[already]]]");
    }

    #[test]
    fn warnings_flag_truncation_and_unknown_models() {
        let w = limit_warnings("V4", &"s".repeat(300), "", None, true);
        assert!(w[0].contains("300 chars (limit 200 for V4)"));
        let w = limit_warnings("V9", "x", "", None, true);
        assert!(w[0].contains("unknown model 'V9'"));
        let w = limit_warnings("V5", "x", &"l".repeat(4500), None, true);
        assert!(w[0].contains("stability target"));
        let w = limit_warnings("V5", "x", "", None, false);
        assert!(w[0].contains("no lyrics on a vocal song"));
        assert!(limit_warnings("V5", "x", "hi", None, false).is_empty());
    }
}
