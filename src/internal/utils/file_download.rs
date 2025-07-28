struct DownloadFileMetadata {
    etag: Option<String>,
    last_modified: Option<String>,
}

fn download_and_cache_file(
    url: &str,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let metadata_path = output_path.with_extension("metadata");

    let client = reqwest::blocking::Client::new();
    let mut request_builder = client.get(url);

    if output_path.exists() && metadata_path.exists() {
        let metadata: DownloadFileMetadata = serde_json::from_reader(
            std::fs::File::open(&metadata_path)
                .map_err(|e| format!("Failed to open metadata file: {}", e))?,
        )
        .map_err(|e| format!("Failed to deserialize metadata: {}", e))?;

        if let Some(etag) = metadata.etag {
            request_builder = request_builder.header("If-None-Match", etag);
        }

        if let Some(last_modified) = metadata.last_modified {
            request_builder = request_builder.header("If-Modified-Since", last_modified);
        }
    }

    let response = request_builder
        .send()
        .map_err(|e| format!("Request failed: {}", e))?;

    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        println!("File not modified, skipping download.");
        return Ok(());
    }

    if !response.status().is_success() {
        return Err(format!("Request failed with status: {}", response.status()).into());
    }

    let mut file = std::fs::File::create(output_path)
        .map_err(|e| format!("Failed to create output file: {}", e))?;
    let content = response
        .bytes()
        .map_err(|e| format!("Failed to read response body: {}", e))?;
    file.write_all(&content)
        .map_err(|e| format!("Failed to write to output file: {}", e))?;

    let etag = response
        .headers()
        .get("ETag")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let last_modified = response
        .headers()
        .get("Last-Modified")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let metadata = DownloadFileMetadata {
        etag,
        last_modified,
    };
    serde_json::to_writer(
        std::fs::File::create(&metadata_path)
            .map_err(|e| format!("Failed to create metadata file: {}", e))?,
        &metadata,
    )
    .map_err(|e| format!("Failed to serialize metadata: {}", e))?;

    Ok(())
}
