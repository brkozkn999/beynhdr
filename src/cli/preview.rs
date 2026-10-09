//! `herdr preview` — drive the read-only preview pane API (`preview.open`,
//! `preview.set`, `preview.close`). This is also the surface plugin processes
//! use to render text/markdown without a PTY.

use crate::api::schema::{
    Method, PreviewCloseParams, PreviewContentType, PreviewOpenParams, PreviewPanePlacement,
    PreviewSetParams, Request, SplitDirection,
};

const PREVIEW_OPEN_USAGE: &str = "usage: herdr preview open --title <title> (--content <text> | --file <path>) [--type text|markdown] [--placement overlay|split|tab] [--pane <pane_id>] [--direction right|down] [--workspace <workspace_id>] [--focus]";
const PREVIEW_SET_USAGE: &str =
    "usage: herdr preview set <pane_id> [--content <text> | --file <path>] [--type text|markdown] [--title <title>]";
const PREVIEW_CLOSE_USAGE: &str = "usage: herdr preview close <pane_id>";

pub(super) fn run_preview_command(args: &[String]) -> std::io::Result<i32> {
    match args.first().map(String::as_str) {
        Some("open") => preview_open(&args[1..]),
        Some("set") => preview_set(&args[1..]),
        Some("close") => preview_close(&args[1..]),
        Some("help" | "--help" | "-h") => {
            print_preview_help();
            Ok(0)
        }
        _ => {
            print_preview_help();
            Ok(2)
        }
    }
}

fn preview_open(args: &[String]) -> std::io::Result<i32> {
    if matches!(
        args.first().map(String::as_str),
        Some("help" | "--help" | "-h")
    ) {
        eprintln!("{PREVIEW_OPEN_USAGE}");
        return Ok(0);
    }
    let mut title = None;
    let mut content = None;
    let mut content_file = None;
    let mut content_type = None;
    let mut placement = None;
    let mut target_pane_id = None;
    let mut direction = None;
    let mut workspace_id = None;
    let mut focus = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--title" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --title");
                    eprintln!("{PREVIEW_OPEN_USAGE}");
                    return Ok(2);
                };
                title = Some(value.clone());
                index += 2;
            }
            "--content" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --content");
                    eprintln!("{PREVIEW_OPEN_USAGE}");
                    return Ok(2);
                };
                content = Some(value.clone());
                index += 2;
            }
            "--file" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --file");
                    eprintln!("{PREVIEW_OPEN_USAGE}");
                    return Ok(2);
                };
                content_file = Some(value.clone());
                index += 2;
            }
            "--type" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --type");
                    eprintln!("{PREVIEW_OPEN_USAGE}");
                    return Ok(2);
                };
                let Some(parsed) = parse_content_type(value) else {
                    eprintln!("invalid --type value: {value} (expected text|markdown)");
                    return Ok(2);
                };
                content_type = Some(parsed);
                index += 2;
            }
            "--placement" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --placement");
                    eprintln!("{PREVIEW_OPEN_USAGE}");
                    return Ok(2);
                };
                let Some(parsed) = parse_placement(value) else {
                    eprintln!("invalid --placement value: {value} (expected overlay|split|tab)");
                    return Ok(2);
                };
                placement = Some(parsed);
                index += 2;
            }
            "--pane" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --pane");
                    eprintln!("{PREVIEW_OPEN_USAGE}");
                    return Ok(2);
                };
                target_pane_id = Some(super::normalize_pane_id(value));
                index += 2;
            }
            "--direction" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --direction");
                    eprintln!("{PREVIEW_OPEN_USAGE}");
                    return Ok(2);
                };
                direction = match value.as_str() {
                    "right" => Some(SplitDirection::Right),
                    "down" => Some(SplitDirection::Down),
                    _ => {
                        eprintln!("invalid --direction value: {value} (expected right|down)");
                        return Ok(2);
                    }
                };
                index += 2;
            }
            "--workspace" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --workspace");
                    eprintln!("{PREVIEW_OPEN_USAGE}");
                    return Ok(2);
                };
                workspace_id = Some(super::normalize_workspace_id(value));
                index += 2;
            }
            "--focus" => {
                focus = true;
                index += 1;
            }
            other => {
                eprintln!("unknown option: {other}");
                eprintln!("{PREVIEW_OPEN_USAGE}");
                return Ok(2);
            }
        }
    }

    let Some(title) = title else {
        eprintln!("missing --title");
        eprintln!("{PREVIEW_OPEN_USAGE}");
        return Ok(2);
    };
    if content.is_some() && content_file.is_some() {
        eprintln!("provide either --content or --file, not both");
        eprintln!("{PREVIEW_OPEN_USAGE}");
        return Ok(2);
    }
    if content.is_none() && content_file.is_none() {
        eprintln!("missing --content or --file");
        eprintln!("{PREVIEW_OPEN_USAGE}");
        return Ok(2);
    }

    super::print_response(&super::send_request(&Request {
        id: "cli:preview:open".into(),
        method: Method::PreviewOpen(PreviewOpenParams {
            title,
            content_type: content_type.unwrap_or_default(),
            content,
            content_file,
            placement,
            workspace_id,
            target_pane_id,
            direction,
            focus,
        }),
    })?)
}

fn preview_set(args: &[String]) -> std::io::Result<i32> {
    if matches!(
        args.first().map(String::as_str),
        Some("help" | "--help" | "-h")
    ) {
        eprintln!("{PREVIEW_SET_USAGE}");
        return Ok(0);
    }
    let Some(raw_pane_id) = args.first() else {
        eprintln!("{PREVIEW_SET_USAGE}");
        return Ok(2);
    };
    let pane_id = super::normalize_pane_id(raw_pane_id);
    let mut content = None;
    let mut content_file = None;
    let mut content_type = None;
    let mut title = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--content" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --content");
                    eprintln!("{PREVIEW_SET_USAGE}");
                    return Ok(2);
                };
                content = Some(value.clone());
                index += 2;
            }
            "--file" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --file");
                    eprintln!("{PREVIEW_SET_USAGE}");
                    return Ok(2);
                };
                content_file = Some(value.clone());
                index += 2;
            }
            "--type" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --type");
                    eprintln!("{PREVIEW_SET_USAGE}");
                    return Ok(2);
                };
                let Some(parsed) = parse_content_type(value) else {
                    eprintln!("invalid --type value: {value} (expected text|markdown)");
                    return Ok(2);
                };
                content_type = Some(parsed);
                index += 2;
            }
            "--title" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --title");
                    eprintln!("{PREVIEW_SET_USAGE}");
                    return Ok(2);
                };
                title = Some(value.clone());
                index += 2;
            }
            other => {
                eprintln!("unknown option: {other}");
                eprintln!("{PREVIEW_SET_USAGE}");
                return Ok(2);
            }
        }
    }
    if content.is_some() && content_file.is_some() {
        eprintln!("provide either --content or --file, not both");
        eprintln!("{PREVIEW_SET_USAGE}");
        return Ok(2);
    }
    if content.is_none() && content_file.is_none() && content_type.is_none() && title.is_none() {
        eprintln!("nothing to set: pass --content/--file, --type, or --title");
        eprintln!("{PREVIEW_SET_USAGE}");
        return Ok(2);
    }

    super::print_response(&super::send_request(&Request {
        id: "cli:preview:set".into(),
        method: Method::PreviewSet(PreviewSetParams {
            pane_id,
            content,
            content_file,
            content_type,
            title,
        }),
    })?)
}

fn preview_close(args: &[String]) -> std::io::Result<i32> {
    if matches!(
        args.first().map(String::as_str),
        Some("help" | "--help" | "-h")
    ) {
        eprintln!("{PREVIEW_CLOSE_USAGE}");
        return Ok(0);
    }
    let Some(raw_pane_id) = args.first() else {
        eprintln!("{PREVIEW_CLOSE_USAGE}");
        return Ok(2);
    };
    if args.len() != 1 {
        eprintln!("{PREVIEW_CLOSE_USAGE}");
        return Ok(2);
    }

    super::print_response(&super::send_request(&Request {
        id: "cli:preview:close".into(),
        method: Method::PreviewClose(PreviewCloseParams {
            pane_id: super::normalize_pane_id(raw_pane_id),
        }),
    })?)
}

fn parse_content_type(value: &str) -> Option<PreviewContentType> {
    match value {
        "text" => Some(PreviewContentType::Text),
        "markdown" => Some(PreviewContentType::Markdown),
        _ => None,
    }
}

fn parse_placement(value: &str) -> Option<PreviewPanePlacement> {
    match value {
        "overlay" => Some(PreviewPanePlacement::Overlay),
        "split" => Some(PreviewPanePlacement::Split),
        "tab" => Some(PreviewPanePlacement::Tab),
        _ => None,
    }
}

fn print_preview_help() {
    eprintln!("herdr preview commands:");
    eprintln!("{PREVIEW_OPEN_USAGE}");
    eprintln!("{PREVIEW_SET_USAGE}");
    eprintln!("{PREVIEW_CLOSE_USAGE}");
}
