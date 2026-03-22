fn main() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let api_key = keyring::Entry::new("auto-crab", "dashscope")
            .unwrap().get_password().unwrap();
        println!("API key loaded: {}...{}", &api_key[..8], &api_key[api_key.len()-4..]);

        // Load and compress image
        let img = image::open("C:\\Users\\zelex\\Desktop\\screen_check.png").unwrap();
        let (w, h) = image::GenericImageView::dimensions(&img);
        println!("Original: {}x{}", w, h);

        let max_dim = 1280u32;
        let img = if w > max_dim || h > max_dim {
            let ratio = max_dim as f64 / w.max(h) as f64;
            let nw = (w as f64 * ratio) as u32;
            let nh = (h as f64 * ratio) as u32;
            img.resize(nw, nh, image::imageops::FilterType::Lanczos3)
        } else {
            img
        };

        let mut buf = std::io::Cursor::new(Vec::new());
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 60);
        img.write_with_encoder(encoder).unwrap();
        let jpeg_data = buf.into_inner();
        println!("Compressed JPEG: {} bytes", jpeg_data.len());

        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &jpeg_data);
        println!("Base64 length: {} chars", b64.len());

        let body = serde_json::json!({
            "model": "qwen3-vl-plus",
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "image_url", "image_url": {"url": format!("data:image/jpeg;base64,{}", b64)}},
                    {"type": "text", "text": "请简单描述这张截图中的内容，50字以内。"}
                ]
            }],
            "max_tokens": 500
        });

        println!("Sending request to DashScope...");
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build().unwrap();

        let resp = client
            .post("https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await;

        match resp {
            Ok(r) => {
                let status = r.status();
                let text = r.text().await.unwrap_or_default();
                println!("Status: {}", status);
                if text.len() > 2000 {
                    println!("Response: {}...", &text[..2000]);
                } else {
                    println!("Response: {}", text);
                }
            }
            Err(e) => {
                println!("Request FAILED: {}", e);
                println!("Is connect error: {}", e.is_connect());
                println!("Is timeout: {}", e.is_timeout());
            }
        }
    });
}
