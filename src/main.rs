mod server;

use std::env;
use std::fs;
use std::path::Path;
use std::process;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use rcode::{EccLevel, Profile, RenderOptions, Style, encode, render_png, render_svg};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let command = args.next().ok_or_else(usage)?;

    match command.as_str() {
        "encode" => run_encode(args.collect()),
        "decode" => run_decode(args.collect()),
        "camera-bench" => run_camera_bench(args.collect()),
        "serve" => run_serve(args.collect()),
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        _ => Err(format!("unknown command `{command}`\n\n{}", usage())),
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CameraManifest {
    version: u32,
    samples: Vec<CameraSample>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CameraSample {
    id: String,
    image: String,
    expected: CameraExpected,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CameraExpected {
    decode: bool,
    payload_base64: Option<String>,
    spokes: Option<usize>,
    ecc: Option<String>,
}

fn run_camera_bench(args: Vec<String>) -> Result<(), String> {
    if args.len() != 1 {
        return Err("camera-bench requires exactly one manifest.json path".to_owned());
    }
    let manifest_path = Path::new(&args[0]);
    let bytes = fs::read(manifest_path)
        .map_err(|error| format!("read manifest `{}`: {error}", manifest_path.display()))?;
    let manifest: CameraManifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid manifest `{}`: {error}", manifest_path.display()))?;
    validate_camera_manifest(&manifest)?;

    let root = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let mut durations = Vec::with_capacity(manifest.samples.len());
    let mut positive = 0usize;
    let mut positive_passed = 0usize;
    let mut negative = 0usize;
    let mut negative_passed = 0usize;
    let mut failed = Vec::new();

    for sample in &manifest.samples {
        let path = root.join(&sample.image);
        let image = fs::read(&path)
            .map_err(|error| format!("read sample `{}`: {error}", path.display()))?;
        let started = Instant::now();
        let result = rcode::decode_image(&image);
        durations.push(started.elapsed());
        let passed = match (sample.expected.decode, result) {
            (true, Ok(decoded)) => {
                positive += 1;
                let payload = BASE64
                    .decode(
                        sample
                            .expected
                            .payload_base64
                            .as_deref()
                            .unwrap_or_default(),
                    )
                    .expect("manifest was validated");
                let profile = Profile::new(
                    sample.expected.spokes.expect("manifest was validated"),
                    sample
                        .expected
                        .ecc
                        .as_deref()
                        .unwrap_or_default()
                        .parse::<EccLevel>()
                        .expect("manifest was validated"),
                )
                .expect("manifest was validated");
                let matches = decoded.payload == payload && decoded.profile == profile;
                positive_passed += usize::from(matches);
                matches
            }
            (false, Err(_)) => {
                negative += 1;
                negative_passed += 1;
                true
            }
            (false, Ok(_)) => {
                negative += 1;
                false
            }
            (true, Err(_)) => {
                positive += 1;
                false
            }
        };
        if !passed {
            failed.push(sample.id.as_str());
        }
    }

    durations.sort_unstable();
    let median = median(&durations);
    let p95 = percentile(&durations, 95);
    let rate = if positive == 0 {
        100.0
    } else {
        positive_passed as f64 * 100.0 / positive as f64
    };
    println!("positive: {positive_passed}/{positive} ({rate:.1}%)");
    println!("negative rejected: {negative_passed}/{negative}");
    println!(
        "decode latency: median {:.3} ms, P95 {:.3} ms",
        median.as_secs_f64() * 1000.0,
        p95.as_secs_f64() * 1000.0
    );
    if !failed.is_empty() {
        return Err(format!(
            "sample expectation mismatch: {}",
            failed.join(", ")
        ));
    }
    Ok(())
}

fn validate_camera_manifest(manifest: &CameraManifest) -> Result<(), String> {
    if manifest.version != 1 {
        return Err("manifest version must be 1".to_owned());
    }
    if manifest.samples.is_empty() {
        return Err("manifest samples must not be empty".to_owned());
    }
    let mut ids = std::collections::HashSet::new();
    for sample in &manifest.samples {
        if sample.id.trim().is_empty() || !ids.insert(&sample.id) {
            return Err("sample IDs must be non-empty and unique".to_owned());
        }
        let image = Path::new(&sample.image);
        if sample.image.is_empty()
            || image.is_absolute()
            || image.components().any(|part| {
                matches!(
                    part,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            return Err(format!(
                "sample `{}` image must be a relative path within the manifest directory",
                sample.id
            ));
        }
        if sample.expected.decode {
            let payload = sample
                .expected
                .payload_base64
                .as_deref()
                .ok_or_else(|| format!("sample `{}` needs payload_base64", sample.id))?;
            BASE64
                .decode(payload)
                .map_err(|_| format!("sample `{}` payload_base64 is invalid", sample.id))?;
            let spokes = sample
                .expected
                .spokes
                .ok_or_else(|| format!("sample `{}` needs spokes", sample.id))?;
            let ecc = sample
                .expected
                .ecc
                .as_deref()
                .ok_or_else(|| format!("sample `{}` needs ecc", sample.id))?;
            Profile::new(spokes, ecc.parse::<EccLevel>().map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        } else if sample.expected.payload_base64.is_some()
            || sample.expected.spokes.is_some()
            || sample.expected.ecc.is_some()
        {
            return Err(format!(
                "negative sample `{}` must not specify payload or profile",
                sample.id
            ));
        }
    }
    Ok(())
}

fn median(values: &[Duration]) -> Duration {
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2
    } else {
        values[middle]
    }
}

fn percentile(values: &[Duration], percentile: usize) -> Duration {
    values[(values.len() * percentile).div_ceil(100).saturating_sub(1)]
}

fn run_encode(args: Vec<String>) -> Result<(), String> {
    let mut text = None;
    let mut output = None;
    let mut options = RenderOptions::default();
    let mut spokes = 72;
    let mut ecc = EccLevel::M;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--spokes" => {
                spokes = next_value(&args, &mut index, "--spokes")?
                    .parse::<usize>()
                    .map_err(|_| "--spokes must be 36, 54 or 72".to_owned())?
            }
            "--ecc" => {
                ecc = next_value(&args, &mut index, "--ecc")?
                    .parse::<EccLevel>()
                    .map_err(|e| e.to_string())?
            }
            "--text" => text = Some(next_value(&args, &mut index, "--text")?),
            "--output" => output = Some(next_value(&args, &mut index, "--output")?),
            "--size" => {
                let value = next_value(&args, &mut index, "--size")?;
                options.size = value
                    .parse::<u32>()
                    .map_err(|_| "--size must be an integer between 384 and 2048".to_owned())?;
            }
            "--style" => options.style = parse_style(&next_value(&args, &mut index, "--style")?)?,
            "--foreground" => {
                options.foreground = parse_color(&next_value(&args, &mut index, "--foreground")?)?;
            }
            "--logo" => {
                let path = next_value(&args, &mut index, "--logo")?;
                options.logo =
                    Some(fs::read(&path).map_err(|error| format!("read logo `{path}`: {error}"))?);
            }
            value => return Err(format!("unknown encode option `{value}`\n\n{}", usage())),
        }
        index += 1;
    }

    let text = text.ok_or_else(|| "encode requires --text".to_owned())?;
    let output = output.ok_or_else(|| "encode requires --output".to_owned())?;
    if !(384..=2048).contains(&options.size) {
        return Err("--size must be between 384 and 2048".to_owned());
    }
    let code = encode(
        text.as_bytes(),
        Profile::new(spokes, ecc).map_err(|e| e.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let extension = Path::new(&output)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" => {
            let png = render_png(&code, &options).map_err(|error| error.to_string())?;
            fs::write(&output, png).map_err(|error| format!("write `{output}`: {error}"))?;
        }
        "svg" => {
            let svg = render_svg(&code, &options).map_err(|error| error.to_string())?;
            fs::write(&output, svg.as_bytes())
                .map_err(|error| format!("write `{output}`: {error}"))?;
        }
        _ => return Err("--output must end in .png or .svg".to_owned()),
    }
    Ok(())
}

fn run_decode(args: Vec<String>) -> Result<(), String> {
    if args.len() != 1 {
        return Err("decode requires exactly one PNG or JPEG path".to_owned());
    }
    let path = &args[0];
    let image = fs::read(path).map_err(|error| format!("read `{path}`: {error}"))?;
    let decoded = rcode::decode_image(&image).map_err(|error| error.to_string())?;
    if let Ok(text) = std::str::from_utf8(&decoded.payload) {
        println!("{text}");
    } else {
        println!("{}", BASE64.encode(&decoded.payload));
    }
    Ok(())
}

fn run_serve(args: Vec<String>) -> Result<(), String> {
    let mut port = 8787u16;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--port" => {
                let value = next_value(&args, &mut index, "--port")?;
                port = value
                    .parse::<u16>()
                    .map_err(|_| "--port must be between 1 and 65535".to_owned())?;
                if port == 0 {
                    return Err("--port must be between 1 and 65535".to_owned());
                }
            }
            value => return Err(format!("unknown serve option `{value}`\n\n{}", usage())),
        }
        index += 1;
    }
    server::run(port).map_err(|error| error.to_string())
}

fn next_value(args: &[String], index: &mut usize, option: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{option} requires a value"))
}

fn parse_style(value: &str) -> Result<Style, String> {
    match value.to_ascii_lowercase().as_str() {
        "dots" => Ok(Style::Dots),
        "lines" => Ok(Style::Lines),
        _ => Err("--style must be `lines` or `dots`".to_owned()),
    }
}

fn parse_color(value: &str) -> Result<[u8; 3], String> {
    let bytes = value.strip_prefix('#').unwrap_or(value).as_bytes();
    if bytes.len() != 6 {
        return Err("--foreground must be a six-digit hexadecimal color".to_owned());
    }
    let mut color = [0; 3];
    for (index, channel) in color.iter_mut().enumerate() {
        let high = hex_nibble(bytes[index * 2]);
        let low = hex_nibble(bytes[index * 2 + 1]);
        *channel = match (high, low) {
            (Some(high), Some(low)) => (high << 4) | low,
            _ => return Err("--foreground must be a six-digit hexadecimal color".to_owned()),
        };
    }
    Ok(color)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn usage() -> String {
    "usage: rcode <encode|decode|camera-bench|serve> [options]".to_owned()
}

fn print_usage() {
    println!(
        "rcode\n\n\
         Encode: rcode encode --text TEXT --output code.png [--spokes 72] [--ecc L|M|Q|H] [--size 768] [--style lines|dots] [--foreground #111827] [--logo logo.png]\n\
         Decode: rcode decode code.png\n\
         Camera benchmark: rcode camera-bench path/to/manifest.json\n\
         Serve:  rcode serve [--port 8787]\n"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parse_color_rejects_non_ascii_without_slicing() {
        assert!(parse_color("你abc").is_err());
        assert!(parse_color("#你abc").is_err());
    }

    #[test]
    fn validates_manifest_expectations_and_relative_paths() {
        let valid: CameraManifest = serde_json::from_str(r#"{"version":1,"samples":[{"id":"positive","image":"sample.png","expected":{"decode":true,"payload_base64":"aGk=","spokes":72,"ecc":"M"}},{"id":"negative","image":"blank.png","expected":{"decode":false}}]}"#).unwrap();
        assert!(validate_camera_manifest(&valid).is_ok());
        let traversal: CameraManifest = serde_json::from_str(r#"{"version":1,"samples":[{"id":"bad","image":"../outside.png","expected":{"decode":false}}]}"#).unwrap();
        assert!(validate_camera_manifest(&traversal).is_err());
        let wrong_version: CameraManifest = serde_json::from_str(
            r#"{"version":2,"samples":[{"id":"x","image":"x.png","expected":{"decode":false}}]}"#,
        )
        .unwrap();
        assert!(validate_camera_manifest(&wrong_version).is_err());
    }

    #[test]
    fn computes_median_and_nearest_rank_p95() {
        let values = [
            Duration::from_millis(1),
            Duration::from_millis(2),
            Duration::from_millis(3),
            Duration::from_millis(4),
        ];
        assert_eq!(
            median(&values),
            Duration::from_millis(2) + Duration::from_micros(500)
        );
        assert_eq!(percentile(&values, 95), Duration::from_millis(4));
        assert_eq!(median(&values[..3]), Duration::from_millis(2));
    }

    #[test]
    fn corpus_runner_accepts_generated_positive_and_rejects_negative() {
        let root = std::env::temp_dir().join(format!(
            "rcode-camera-bench-{}-{}",
            process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let image = render_png(
            &encode(b"generated fixture", Profile::default()).unwrap(),
            &RenderOptions::default(),
        )
        .unwrap();
        fs::write(root.join("positive.png"), image).unwrap();
        let blank = image::GrayImage::from_pixel(384, 384, image::Luma([255]));
        let mut blank_png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageLuma8(blank)
            .write_to(&mut blank_png, image::ImageFormat::Png)
            .unwrap();
        fs::write(root.join("negative.png"), blank_png.into_inner()).unwrap();
        let manifest = r#"{"version":1,"samples":[{"id":"generated-positive","image":"positive.png","expected":{"decode":true,"payload_base64":"Z2VuZXJhdGVkIGZpeHR1cmU=","spokes":72,"ecc":"M"}},{"id":"generated-negative","image":"negative.png","expected":{"decode":false}}]}"#;
        let manifest_path = root.join("manifest.json");
        fs::write(&manifest_path, manifest).unwrap();
        assert!(run_camera_bench(vec![manifest_path.to_string_lossy().into_owned()]).is_ok());
        let mismatch = manifest.replace("Z2VuZXJhdGVkIGZpeHR1cmU=", "d3Jvbmc=");
        fs::write(&manifest_path, mismatch).unwrap();
        assert!(run_camera_bench(vec![manifest_path.to_string_lossy().into_owned()]).is_err());
        fs::write(&manifest_path, r#"{"version":1,"samples":[{"id":"false-negative","image":"positive.png","expected":{"decode":false}}]}"#).unwrap();
        assert!(run_camera_bench(vec![manifest_path.to_string_lossy().into_owned()]).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
