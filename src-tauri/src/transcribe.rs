use anyhow::{Context, Result};
use base64::Engine;
use reqwest::multipart;

/// Validate API key by sending a tiny silent WAV to the transcription endpoint.
pub async fn validate_api_key(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    base_url: &str,
) -> Result<()> {
    if model.trim().is_empty() {
        anyhow::bail!("Model is required");
    }

    let wav = generate_silent_wav();
    let file_part = multipart::Part::bytes(wav)
        .file_name("test.wav")
        .mime_str("audio/wav")?;
    let form = multipart::Form::new()
        .part("file", file_part)
        .text("model", model.to_string());

    let resp = client
        .post(openai_transcription_url(base_url)?)
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await
        .context("Network error")?;

    if resp.status() == 401 {
        anyhow::bail!("Invalid API key");
    }
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("{}", body);
    }
    Ok(())
}

/// Generate a minimal valid WAV file (0.5s silence, 16kHz mono 16-bit).
fn generate_silent_wav() -> Vec<u8> {
    let sample_rate: u32 = 16000;
    let num_samples: u32 = sample_rate / 2; // 0.5 seconds
    let data_size = num_samples * 2; // 16-bit = 2 bytes per sample
    let file_size = 36 + data_size;

    let mut buf = Vec::with_capacity(file_size as usize + 8);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&file_size.to_le_bytes());
    buf.extend_from_slice(b"WAVEfmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&1u16.to_le_bytes()); // mono
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    buf.extend_from_slice(&2u16.to_le_bytes()); // block align
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());
    buf.resize(buf.len() + data_size as usize, 0); // silence
    buf
}

pub async fn transcribe_audio(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    base_url: &str,
    wav_data: Vec<u8>,
    language: Option<&str>,
) -> Result<String> {
    if model.trim().is_empty() {
        anyhow::bail!("Model is required");
    }

    let file_part = multipart::Part::bytes(wav_data)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;

    let mut form = multipart::Form::new()
        .part("file", file_part)
        .text("model", model.to_string());

    if let Some(lang) = language {
        if lang != "auto" {
            // OpenAI only accepts ISO 639-1 codes; map zh-Hans/zh-Hant to zh
            let code = match lang {
                "zh-Hans" | "zh-Hant" => "zh",
                other => other,
            };
            form = form.text("language", code.to_string());
        }
    }

    let resp = client
        .post(openai_transcription_url(base_url)?)
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await
        .context("Failed to send transcription request")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("API error {}: {}", status, body);
    }

    let json: serde_json::Value = resp.json().await.context("Failed to parse API response")?;
    let text = json["text"]
        .as_str()
        .context("Missing 'text' field in response")?
        .to_string();

    Ok(text)
}

fn openai_transcription_url(base_url: &str) -> Result<String> {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        anyhow::bail!("Base URL is required");
    }

    let base = trimmed.trim_end_matches('/');
    let url = if base.ends_with("/audio/transcriptions") {
        base.to_string()
    } else {
        format!("{}/audio/transcriptions", base)
    };

    let parsed = reqwest::Url::parse(&url).context("Invalid OpenAI-compatible Base URL")?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed.to_string()),
        scheme => anyhow::bail!("Unsupported Base URL scheme: {}", scheme),
    }
}

// ── Google Gemini (generateContent) ─────────────────────────────────

const GEMINI_INLINE_LIMIT: usize = 100 * 1024 * 1024; // 100 MB (raised from 20 MB, 2026-01-08)

/// Validate Gemini API key by sending a tiny silent WAV.
pub async fn validate_gemini_api_key(client: &reqwest::Client, api_key: &str) -> Result<()> {
    let wav = generate_silent_wav();
    let b64 = base64::engine::general_purpose::STANDARD.encode(&wav);

    let body = serde_json::json!({
        "contents": [{
            "parts": [
                {"text": "Transcribe the following audio. Output only the transcribed text, nothing else."},
                {"inline_data": {"mime_type": "audio/wav", "data": b64}}
            ]
        }]
    });

    let resp = client
        .post("https://generativelanguage.googleapis.com/v1beta/models/gemini-3-flash-preview:generateContent")
        .header("x-goog-api-key", api_key)
        .json(&body)
        .send()
        .await
        .context("Network error")?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        if body.contains("API_KEY_INVALID")
            || body.contains("PERMISSION_DENIED")
            || status.as_u16() == 401
        {
            anyhow::bail!("Invalid API key");
        }
        anyhow::bail!("Gemini API error {}: {}", status, body);
    }
    Ok(())
}

pub async fn transcribe_gemini(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    wav_data: Vec<u8>,
    language: Option<&str>,
) -> Result<String> {
    // Check inline size limit (base64 expands ~33%)
    let estimated_b64_size = (wav_data.len() * 4 + 2) / 3;
    if estimated_b64_size > GEMINI_INLINE_LIMIT {
        anyhow::bail!(
            "Audio file too large for Gemini ({:.1} MB). Maximum is ~75 MB WAV.",
            wav_data.len() as f64 / 1_048_576.0
        );
    }

    let b64 = base64::engine::general_purpose::STANDARD.encode(&wav_data);

    let prompt = match language {
        // Gemini has no language param for audio; it tends to drift to
        // Traditional characters on Mandarin audio unless the output
        // script is an explicit hard requirement in the prompt.
        Some("zh-Hans") => "Transcribe the following audio. Output only the transcribed text, nothing else. Use Simplified Chinese (简体中文). Do NOT use Traditional Chinese characters.".to_string(),
        Some(lang) if lang != "auto" => {
            let lang_name = language_code_to_name(lang);
            format!(
                "Transcribe the following audio. Output only the transcribed text, nothing else. The language is {}.",
                lang_name
            )
        }
        _ => "Transcribe the following audio. Output only the transcribed text, nothing else."
            .to_string(),
    };

    let body = serde_json::json!({
        "contents": [{
            "parts": [
                {"text": prompt},
                {"inline_data": {"mime_type": "audio/wav", "data": b64}}
            ]
        }]
    });

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        model
    );

    let resp = client
        .post(&url)
        .header("x-goog-api-key", api_key)
        .json(&body)
        .send()
        .await
        .context("Failed to send transcription request")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Gemini API error {}: {}", status, body);
    }

    let json: serde_json::Value = resp.json().await.context("Failed to parse API response")?;

    // Check for content filtering / prompt block
    if let Some(reason) = json["promptFeedback"]["blockReason"].as_str() {
        anyhow::bail!("Gemini blocked the request: {}", reason);
    }

    // Gemini 3.7 Flash sometimes leaks its chain-of-thought as an extra
    // leading text part with thought=false (verified 2026-08-17); the clean
    // transcript is always the last part. Skip thought parts, take the last.
    let text = json["candidates"][0]["content"]["parts"]
        .as_array()
        .and_then(|parts| {
            parts
                .iter()
                .filter(|p| !p["thought"].as_bool().unwrap_or(false))
                .filter_map(|p| p["text"].as_str())
                .last()
        })
        .unwrap_or_default()
        .to_string();

    if text.is_empty() {
        anyhow::bail!("Gemini returned empty transcription");
    }

    Ok(text.trim().to_string())
}

fn language_code_to_name(code: &str) -> &str {
    match code {
        "zh" => "Chinese",
        "zh-Hans" => "Simplified Chinese",
        "zh-Hant" => "Traditional Chinese",
        "en" => "English",
        "ja" => "Japanese",
        "ko" => "Korean",
        "es" => "Spanish",
        "fr" => "French",
        "de" => "German",
        _ => code,
    }
}

#[cfg(test)]
mod tests {
    use super::openai_transcription_url;

    #[test]
    fn builds_transcription_url_from_base_url() {
        let url = openai_transcription_url("https://api.example.com/v1").unwrap();
        assert_eq!(url, "https://api.example.com/v1/audio/transcriptions");
    }

    #[test]
    fn accepts_full_transcription_endpoint() {
        let url =
            openai_transcription_url("https://api.example.com/v1/audio/transcriptions/").unwrap();
        assert_eq!(url, "https://api.example.com/v1/audio/transcriptions");
    }

    #[test]
    fn rejects_empty_base_url() {
        let err = openai_transcription_url("  ").unwrap_err().to_string();
        assert!(err.contains("Base URL is required"));
    }

    #[test]
    fn rejects_unsupported_base_url_scheme() {
        let err = openai_transcription_url("socks5://127.0.0.1:1080")
            .unwrap_err()
            .to_string();
        assert!(err.contains("Unsupported Base URL scheme"));
    }
}
