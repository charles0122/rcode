use std::error::Error as StdError;
use std::io::{Cursor, Read};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use rcode::{
    EccLevel, MAX_PAYLOAD, Profile, RenderOptions, Style, encode_with_profile, render_png,
    render_svg,
};
use serde::{Deserialize, Serialize};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

#[cfg(test)]
use tiny_http::TestRequest;

type HttpResponse = Response<Cursor<Vec<u8>>>;

const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
const MAX_IMAGE_PIXELS: u64 = 16_000_000;
const INDEX_HTML: &[u8] = include_bytes!("../web/index.html");
const STYLE_CSS: &[u8] = include_bytes!("../web/style.css");
const APP_JS: &[u8] = include_bytes!("../web/app.js");

pub fn run(port: u16) -> Result<(), Box<dyn StdError + Send + Sync>> {
    if port == 0 {
        return Err("port must be between 1 and 65535".into());
    }
    let server = Server::http(("127.0.0.1", port))?;
    println!("rcode workbench: http://127.0.0.1:{port}/");
    for request in server.incoming_requests() {
        // ponytail: one synchronous worker keeps the local one-user server small; add a pool if throughput matters.
        handle_request(request, port);
    }
    Ok(())
}

fn handle_request(mut request: Request, port: u16) {
    let response = match validate_request(&request, port).and_then(|_| route_request(&mut request))
    {
        Ok(response) => response,
        Err(response) => response,
    };
    let _ = request.respond(response);
}

fn validate_request(request: &Request, port: u16) -> Result<(), HttpResponse> {
    if !host_allowed(request, port) {
        return Err(json_error(400, "invalid host"));
    }
    if let Some(origin) = header_value(request, "Origin") {
        if !origin_allowed(origin, port) {
            return Err(json_error(403, "foreign origin is not allowed"));
        }
    }
    if let Some(content_length) = header_value(request, "Content-Length") {
        match content_length.parse::<usize>() {
            Ok(length) if length <= MAX_BODY_BYTES => {}
            Ok(_) => return Err(json_error(413, "request body exceeds 8 MiB")),
            Err(_) => return Err(json_error(400, "invalid content length")),
        }
    }
    Ok(())
}

fn route_request(request: &mut Request) -> Result<HttpResponse, HttpResponse> {
    let method = request.method().clone();
    let path = request.url().split('?').next().unwrap_or(request.url());
    match (method, path) {
        (Method::Get, "/api/profiles") => {
            let profiles: Vec<_> = [36, 54, 72]
                .into_iter()
                .flat_map(|spokes| {
                    EccLevel::ALL
                        .into_iter()
                        .map(move |level| ProfileInfo::from(Profile::new(spokes, level).unwrap()))
                })
                .collect();
            json_response(200, &profiles)
        }
        (Method::Get, "/") => Ok(Response::from_data(INDEX_HTML.to_vec())
            .with_header(content_type("text/html; charset=utf-8"))),
        (Method::Get, "/style.css") => Ok(Response::from_data(STYLE_CSS.to_vec())
            .with_header(content_type("text/css; charset=utf-8"))),
        (Method::Get, "/app.js") => Ok(Response::from_data(APP_JS.to_vec())
            .with_header(content_type("text/javascript; charset=utf-8"))),
        (Method::Get, _) => Err(status_response(404, "not found")),
        (Method::Post, "/api/encode") => handle_encode(request),
        (Method::Post, "/api/decode") => handle_decode(request),
        (Method::Post, _) => Err(status_response(404, "not found")),
        _ => Err(status_response_with_allow(
            405,
            "method not allowed",
            "GET, POST",
        )),
    }
}

fn handle_encode(request: &mut Request) -> Result<HttpResponse, HttpResponse> {
    let body = read_body(request)?;
    let input: EncodeRequest =
        serde_json::from_slice(&body).map_err(|_| json_error(400, "invalid JSON payload"))?;
    let text = input
        .text
        .ok_or_else(|| json_error(400, "text is required"))?;
    if text.len() > MAX_PAYLOAD {
        return Err(json_error(400, "payload exceeds the 128-byte limit"));
    }
    let size = input.size.unwrap_or(768);
    if !(384..=2048).contains(&size) {
        return Err(json_error(400, "size must be between 384 and 2048"));
    }
    let foreground = input
        .foreground
        .as_deref()
        .map(parse_color)
        .transpose()
        .map_err(|_| json_error(400, "foreground must be a six-digit hexadecimal color"))?
        .unwrap_or([0x11, 0x18, 0x27]);
    let style = input
        .style
        .as_deref()
        .map(parse_style)
        .transpose()
        .map_err(|_| json_error(400, "style must be `lines` or `dots`"))?
        .unwrap_or(Style::Lines);
    let logo = input
        .logo_base64
        .as_deref()
        .map(|logo| {
            BASE64
                .decode(logo)
                .map_err(|_| json_error(400, "logo_base64 is invalid"))
        })
        .transpose()?
        .filter(|logo| !logo.is_empty());
    if logo
        .as_ref()
        .is_some_and(|logo| logo.len() > MAX_BODY_BYTES)
    {
        return Err(json_error(413, "logo exceeds 8 MiB"));
    }

    let profile = Profile::new(
        input.spokes.unwrap_or(72),
        input
            .ecc
            .as_deref()
            .unwrap_or("M")
            .parse()
            .map_err(|e: rcode::Error| json_error(400, &e.to_string()))?,
    )
    .map_err(|e| json_error(400, &e.to_string()))?;
    let options = RenderOptions {
        size,
        foreground,
        style,
        logo,
    };
    let code = encode_with_profile(text.as_bytes(), profile)
        .map_err(|error| json_error(400, &error.to_string()))?;
    let png = render_png(&code, &options).map_err(|error| json_error(400, &error.to_string()))?;
    let svg = render_svg(&code, &options).map_err(|error| json_error(400, &error.to_string()))?;
    json_response(
        200,
        &EncodeResponse {
            png_base64: BASE64.encode(png),
            svg,
            byte_length: text.len(),
            profile: ProfileInfo::from(profile),
        },
    )
}

fn handle_decode(request: &mut Request) -> Result<HttpResponse, HttpResponse> {
    let body = read_body(request)?;
    match image::guess_format(&body) {
        Ok(image::ImageFormat::Png | image::ImageFormat::Jpeg) => {}
        Ok(_) | Err(_) => return Err(status_response(415, "unsupported image format")),
    }
    if let Ok(reader) = image::ImageReader::new(Cursor::new(&body)).with_guessed_format() {
        if let Ok((width, height)) = reader.into_dimensions() {
            if u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
                return Err(status_response(413, "image exceeds 16 megapixels"));
            }
        }
    }
    let decoded = rcode::decode_image(&body)
        .map_err(|_| status_response(422, "unable to decode radial code"))?;
    let text = std::str::from_utf8(&decoded.payload)
        .ok()
        .map(str::to_owned);
    let landmarks = decoded
        .landmarks
        .iter()
        .map(|landmark| LandmarkResponse {
            x: landmark.x,
            y: landmark.y,
        })
        .collect();
    json_response(
        200,
        &DecodeResponse {
            text,
            payload_base64: BASE64.encode(&decoded.payload),
            byte_length: decoded.payload.len(),
            corrected_bytes: decoded.corrected_bytes,
            landmarks,
            width: decoded.width,
            height: decoded.height,
            profile: decoded.profile.map(ProfileInfo::from),
        },
    )
}

fn read_body(request: &mut Request) -> Result<Vec<u8>, HttpResponse> {
    let mut body = Vec::new();
    let mut reader = request.as_reader().take((MAX_BODY_BYTES + 1) as u64);
    reader
        .read_to_end(&mut body)
        .map_err(|_| status_response(400, "unable to read request body"))?;
    if body.len() > MAX_BODY_BYTES {
        return Err(json_error(413, "request body exceeds 8 MiB"));
    }
    Ok(body)
}

fn json_response<T: Serialize>(status: u16, value: &T) -> Result<HttpResponse, HttpResponse> {
    let body = serde_json::to_vec(value)
        .map_err(|_| status_response(500, "unable to serialize response"))?;
    Ok(Response::from_data(body)
        .with_status_code(StatusCode(status))
        .with_header(content_type("application/json; charset=utf-8")))
}

fn json_error(status: u16, message: &str) -> HttpResponse {
    let body = serde_json::to_vec(&ErrorResponse {
        error: message.to_owned(),
    })
    .unwrap_or_else(|_| b"{\"error\":\"request failed\"}".to_vec());
    Response::from_data(body)
        .with_status_code(StatusCode(status))
        .with_header(content_type("application/json; charset=utf-8"))
}

fn status_response(status: u16, message: &str) -> HttpResponse {
    json_error(status, message)
}

fn status_response_with_allow(status: u16, message: &str, allow: &str) -> HttpResponse {
    status_response(status, message)
        .with_header(Header::from_bytes("Allow", allow).expect("static header"))
}

fn content_type(value: &str) -> Header {
    Header::from_bytes("Content-Type", value).expect("static header")
}

fn header_value<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request
        .headers()
        .iter()
        .find(|header| header.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|header| header.value.as_str())
}

fn host_allowed(request: &Request, port: u16) -> bool {
    let Some(host) = header_value(request, "Host") else {
        return false;
    };
    host.eq_ignore_ascii_case(&format!("localhost:{port}"))
        || host.eq_ignore_ascii_case(&format!("127.0.0.1:{port}"))
}

fn origin_allowed(origin: &str, port: u16) -> bool {
    origin == format!("http://localhost:{port}") || origin == format!("http://127.0.0.1:{port}")
}

fn parse_style(value: &str) -> Result<Style, ()> {
    match value.to_ascii_lowercase().as_str() {
        "dots" => Ok(Style::Dots),
        "lines" => Ok(Style::Lines),
        _ => Err(()),
    }
}

fn parse_color(value: &str) -> Result<[u8; 3], ()> {
    let bytes = value.strip_prefix('#').unwrap_or(value).as_bytes();
    if bytes.len() != 6 {
        return Err(());
    }
    let mut color = [0; 3];
    for (index, channel) in color.iter_mut().enumerate() {
        let high = hex_nibble(bytes[index * 2]).ok_or(())?;
        let low = hex_nibble(bytes[index * 2 + 1]).ok_or(())?;
        *channel = (high << 4) | low;
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

#[derive(Debug, Deserialize)]
struct EncodeRequest {
    spokes: Option<usize>,
    ecc: Option<String>,
    text: Option<String>,
    size: Option<u32>,
    foreground: Option<String>,
    style: Option<String>,
    logo_base64: Option<String>,
}

#[derive(Debug, Serialize)]
struct EncodeResponse {
    profile: ProfileInfo,
    png_base64: String,
    svg: String,
    byte_length: usize,
}

#[derive(Debug, Serialize)]
struct DecodeResponse {
    profile: Option<ProfileInfo>,
    text: Option<String>,
    payload_base64: String,
    byte_length: usize,
    corrected_bytes: usize,
    landmarks: Vec<LandmarkResponse>,
    width: u32,
    height: u32,
}

#[derive(Debug, Serialize)]
struct LandmarkResponse {
    x: f64,
    y: f64,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_loopback_hosts() {
        assert!(host_allowed(
            &request_with_headers("localhost:8787", &[]),
            8787
        ));
        assert!(host_allowed(
            &request_with_headers("127.0.0.1:8787", &[]),
            8787
        ));
        assert!(!host_allowed(
            &request_with_headers("example.com:8787", &[]),
            8787
        ));
    }

    #[test]
    fn rejects_foreign_origins() {
        assert!(origin_allowed("http://localhost:8787", 8787));
        assert!(!origin_allowed("https://example.com", 8787));
    }

    #[test]
    fn rejects_non_ascii_colors_without_slicing() {
        assert!(parse_color("你abc").is_err());
        assert!(parse_color("#你abc").is_err());
    }

    #[test]
    fn serves_static_root_and_rejects_oversized_content_length() {
        let mut root: Request = TestRequest::new()
            .with_method(Method::Get)
            .with_path("/")
            .into();
        let response = match route_request(&mut root) {
            Ok(response) => response,
            Err(_) => panic!("root route should succeed"),
        };
        assert_eq!(response.status_code(), StatusCode(200));

        let oversized = request_with_headers("localhost:8787", &[("Content-Length", "8388609")]);
        assert_eq!(
            validate_request(&oversized, 8787)
                .unwrap_err()
                .status_code(),
            StatusCode(413)
        );
    }

    fn request_with_headers(host: &str, extra: &[(&str, &str)]) -> Request {
        let mut request = TestRequest::new().with_header(Header::from_bytes("Host", host).unwrap());
        for (name, value) in extra {
            request = request.with_header(Header::from_bytes(*name, *value).unwrap());
        }
        request.into()
    }
}

#[derive(Debug, Serialize)]
struct ProfileInfo {
    spokes: usize,
    ecc: String,
    capacity: usize,
    correction_percent: usize,
    correctable_bytes: usize,
    parity_bytes: usize,
}
impl From<Profile> for ProfileInfo {
    fn from(p: Profile) -> Self {
        Self {
            spokes: p.spokes(),
            ecc: p.level.to_string(),
            capacity: p.capacity(),
            correction_percent: p.level.percent(),
            correctable_bytes: p.correctable_bytes(),
            parity_bytes: p.parity_bytes(),
        }
    }
}
