use anyhow::{bail, Context, Result};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LyricWord {
    pub text: String,
    pub start_time: f64,
    pub end_time: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LyricRole {
    Main,
    Duet,
    Background,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LyricLine {
    pub words: Vec<LyricWord>,
    pub start_time: f64,
    pub end_time: f64,
    pub role: LyricRole,
    pub agent: Option<String>,
}

pub type Lyrics = Vec<LyricLine>;

fn parse_time(s: &str) -> Result<f64> {
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        2 => {
            let minutes: f64 = parts[0].parse()?;
            let seconds: f64 = parts[1].parse()?;
            Ok(minutes * 60. + seconds)
        }
        3 => {
            let hours: f64 = parts[0].parse()?;
            let minutes: f64 = parts[1].parse()?;
            let seconds: f64 = parts[2].parse()?;
            Ok(hours * 3600. + minutes * 60. + seconds)
        }
        _ => bail!("invalid time format: {}", s),
    }
}

fn extract_attr<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!("{}=\"", name);
    s.find(&prefix)
        .map(|start| {
            let start = start + prefix.len();
            s[start..].find('"').map(|end| &s[start..start + end])
        })
        .flatten()
}

fn parse_ttml_span(s: &str, default_time: Option<(f64, f64)>) -> Result<LyricWord> {
    let text: &str = {
        let content_start = s.find('>').context("span has no content")? + 1;
        let content_end = s[content_start..].find('<').context("span has no closing tag")?;
        &s[content_start..content_start + content_end]
    };

    let start_time = extract_attr(s, "begin").and_then(|v| parse_time(v).ok()).or(default_time.map(|t| t.0));

    let end_time = extract_attr(s, "end").and_then(|v| parse_time(v).ok()).or(default_time.map(|t| t.1));

    if let (Some(start_time), Some(end_time)) = (start_time, end_time) {
        Ok(LyricWord {
            text: text.to_string(),
            start_time,
            end_time,
        })
    } else {
        Err(anyhow::anyhow!("span has no time attributes and no default time provided"))
    }
}

fn parse_ttml_p(s: &str) -> Result<Vec<LyricLine>> {
    let p_start_time = extract_attr(s, "begin")
        .context("p has no begin attribute")
        .and_then(|v| parse_time(v))
        .context("invalid begin time")?;

    let p_end_time = extract_attr(s, "end")
        .context("p has no end attribute")
        .and_then(|v| parse_time(v))
        .context("invalid end time")?;

    let agent = extract_attr(s, "ttm:agent").map(|v| v.to_string());

    let role = if agent.as_deref() == Some("v1") {
        LyricRole::Main
    } else {
        LyricRole::Duet
    };

    let mut main_words = Vec::new();
    let mut bg_lines = Vec::new();

    let content_start = s.find('>').context("p has no content")? + 1;
    let content_end = s.rfind("</p>").context("p has no closing tag")?;
    let content = &s[content_start..content_end];
    let mut remaining = content;
    let mut in_bg = false;
    let mut bg_words = Vec::new();
    let mut bg_start_time = p_start_time;
    let mut bg_end_time = p_end_time;

    while let Some(start) = remaining.find("<span") {
        let span_str = &remaining[start..];
        let tag_end = span_str.find('>').context("span has no closing >")?;
        let tag_full = &span_str[..tag_end + 1];

        let is_bg = tag_full.contains(r#"ttm:role="x-bg""#);

        if is_bg {
            if in_bg && !bg_words.is_empty() {
                bg_lines.push(LyricLine {
                    words: bg_words,
                    start_time: bg_start_time,
                    end_time: bg_end_time,
                    role: LyricRole::Background,
                    agent: agent.clone(),
                });
            }

            in_bg = true;
            bg_words = Vec::new();

            bg_start_time = extract_attr(tag_full, "begin").and_then(|v| parse_time(v).ok()).unwrap_or(p_start_time);
            bg_end_time = extract_attr(tag_full, "end").and_then(|v| parse_time(v).ok()).unwrap_or(p_end_time);

            remaining = &remaining[start + tag_end + 1..];
        } else {
            let span_end = span_str.find("</span>").context("span has no closing tag")? + 7;
            let full_span = &span_str[..span_end];

            if in_bg {
                let default_time = bg_words
                    .last()
                    .map(|w: &LyricWord| (w.end_time, w.end_time))
                    .or(Some((bg_start_time, bg_end_time)));
                if let Ok(word) = parse_ttml_span(full_span, default_time) {
                    let mut clean_word = word;
                    clean_word.text = clean_word.text.replace(['(', ')'], "");
                    bg_words.push(clean_word);
                }

                let after_span = &remaining[start + span_end..];
                if after_span.starts_with("</span>") {
                    if !bg_words.is_empty() {
                        bg_lines.push(LyricLine {
                            words: bg_words,
                            start_time: bg_start_time,
                            end_time: bg_end_time,
                            role: LyricRole::Background,
                            agent: agent.clone(),
                        });
                    }
                    in_bg = false;
                    bg_words = Vec::new();
                    remaining = &after_span[7..];
                } else {
                    let trimmed = after_span.trim_start();
                    let has_space = after_span.len() != trimmed.len();
                    if has_space {
                        if let Some(end_time) = bg_words.last().map(|w| w.end_time) {
                            bg_words.push(LyricWord {
                                text: " ".to_string(),
                                start_time: end_time,
                                end_time: end_time,
                            });
                        }
                    }
                    remaining = trimmed;
                }
            } else {
                let default_time = main_words
                    .last()
                    .map(|w: &LyricWord| (w.end_time, w.end_time))
                    .or(Some((p_start_time, p_end_time)));
                if let Ok(word) = parse_ttml_span(full_span, default_time) {
                    main_words.push(word);
                }
                let after_span = &remaining[start + span_end..];
                let trimmed = after_span.trim_start();
                let has_space = after_span.len() != trimmed.len();
                if has_space {
                    if let Some(end_time) = main_words.last().map(|w| w.end_time) {
                        main_words.push(LyricWord {
                            text: " ".to_string(),
                            start_time: end_time,
                            end_time: end_time,
                        });
                    }
                }
                remaining = trimmed;
            }
        }
    }

    if in_bg && !bg_words.is_empty() {
        bg_lines.push(LyricLine {
            words: bg_words,
            start_time: bg_start_time,
            end_time: bg_end_time,
            role: LyricRole::Background,
            agent: agent.clone(),
        });
    }

    let mut result = Vec::new();
    if !main_words.is_empty() {
        let mut main_end_time = p_end_time;
        if !bg_lines.is_empty() {
            main_end_time = main_end_time.max(bg_lines[0].end_time);
        }
        result.push(LyricLine {
            words: main_words,
            start_time: p_start_time,
            end_time: main_end_time,
            role,
            agent,
        });
    }

    result.extend(bg_lines);
    Ok(result)
}

pub fn parse_ttml(source: &str) -> Result<Lyrics> {
    let mut lyrics = Vec::new();
    let mut remaining = source;

    while let Some(start) = remaining.find("<p ") {
        let p_end = remaining[start..].find("</p>").context("p has no closing tag")? + 4;
        let p_str = &remaining[start..start + p_end];
        if let Ok(lines) = parse_ttml_p(p_str) {
            lyrics.extend(lines);
        }
        remaining = &remaining[start + p_end..];
    }

    lyrics.sort_by(|a, b| a.start_time.partial_cmp(&b.start_time).unwrap_or(std::cmp::Ordering::Equal));
    Ok(lyrics)
}

pub fn parse_lrc(source: &str) -> Result<Lyrics> {
    let mut lyrics = Vec::new();

    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('[') && !line.contains(']') {
            continue;
        }

        let mut remaining = line;
        let mut times = Vec::new();
        let mut contents = Vec::new();

        while let Some(start) = remaining.find('[') {
            let end = remaining[start..].find(']').context("timestamp not closed")?;
            let timestamp = &remaining[start + 1..start + end];

            if let Ok(time) = parse_time(timestamp) {
                times.push(time);

                let content_start = start + end + 1;
                let content = if let Some(next_bracket) = remaining[content_start..].find('[') {
                    &remaining[content_start..content_start + next_bracket]
                } else {
                    &remaining[content_start..]
                };
                contents.push(content.to_string());
            }

            remaining = &remaining[start + end + 1..];
        }

        if times.is_empty() {
            continue;
        }

        let mut words = Vec::new();
        let line_start = times[0];

        if times.len() > 1 {
            for i in 0..times.len() {
                let start_time = times[i];
                let end_time = times.get(i + 1).copied().unwrap_or(start_time + 2.0);

                let content = &contents[i];
                let chars: Vec<char> = content.chars().collect();

                if chars.is_empty() {
                    continue;
                }

                for (j, c) in chars.iter().enumerate() {
                    let char_start = start_time + j as f64 * (end_time - start_time) / chars.len() as f64;
                    let char_end = if j == chars.len() - 1 {
                        end_time
                    } else {
                        char_start + (end_time - start_time) / chars.len() as f64
                    };

                    words.push(LyricWord {
                        text: c.to_string(),
                        start_time: char_start,
                        end_time: char_end,
                    });
                }
            }

            let line_end = times.last().copied().unwrap_or(line_start + 2.0);
            lyrics.push(LyricLine {
                words,
                start_time: line_start,
                end_time: line_end,
                role: LyricRole::Main,
                agent: None,
            });
        } else {
            let content = contents.join("").trim().to_string();

            if content.is_empty() {
                continue;
            }

            lyrics.push(LyricLine {
                words: vec![LyricWord {
                    text: content,
                    start_time: line_start,
                    end_time: line_start,
                }],
                start_time: line_start,
                end_time: line_start,
                role: LyricRole::Main,
                agent: None,
            });
        }
    }

    lyrics.sort_by(|a, b| a.start_time.partial_cmp(&b.start_time).unwrap_or(std::cmp::Ordering::Equal));

    for i in 0..lyrics.len() - 1 {
        let next_start = lyrics[i + 1].start_time;

        if lyrics[i].end_time <= lyrics[i].start_time {
            lyrics[i].end_time = next_start;
            for word in &mut lyrics[i].words {
                word.end_time = next_start;
            }
        } else if lyrics[i].end_time > next_start {
            lyrics[i].end_time = next_start;
            for word in &mut lyrics[i].words {
                if word.end_time > next_start {
                    word.end_time = next_start;
                }
            }
        }
    }

    Ok(lyrics)
}

pub fn detect_format(source: &str) -> &'static str {
    if source.starts_with('<') {
        "ttml"
    } else if source.contains('[') && source.contains(']') {
        "lrc"
    } else {
        "unknown"
    }
}

pub fn parse(source: &str) -> Result<Lyrics> {
    match detect_format(source) {
        "ttml" => parse_ttml(source),
        "lrc" => parse_lrc(source),
        _ => bail!("unknown lyrics format"),
    }
}
