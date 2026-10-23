mod server;

use std::env;
use std::fs;
use std::path::Path;
use std::process;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use radialcode::{
    EccLevel, MAX_PAYLOAD, Profile, RenderOptions, Style, encode_with_profile, render_png,
    render_svg,
};

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
        "serve" => run_serve(args.collect()),
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(())
        }
        _ => Err(format!("unknown command `{command}`\n\n{}", usage())),
    }
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
    if text.len() > MAX_PAYLOAD {
        return Err(format!(
            "payload is {} bytes; maximum is {MAX_PAYLOAD}",
            text.len()
        ));
    }

    let code = encode_with_profile(
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
    let decoded = radialcode::decode_image(&image).map_err(|error| error.to_string())?;
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
    "usage: radialcode <encode|decode|serve> [options]".to_owned()
}

fn print_usage() {
    println!(
        "radialcode\n\n\
         Encode: radialcode encode --text TEXT --output code.png [--spokes 72] [--ecc L|M|Q|H] [--size 768] [--style lines|dots] [--foreground #111827] [--logo logo.png]\n\
         Decode: radialcode decode code.png\n\
         Serve:  radialcode serve [--port 8787]\n"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_color_rejects_non_ascii_without_slicing() {
        assert!(parse_color("你abc").is_err());
        assert!(parse_color("#你abc").is_err());
    }
}
