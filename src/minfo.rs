use std::io;
use crate::{execute_command_with_output, lazy_regex, mutex_lock, Track};
use crate::cli::ARGS;

lazy_regex!(
    TRACKS_HEADER_REGEX,
    r"Track +Sess +Type +Start Addr +End Addr +Size"
);
lazy_regex!(
    TRACK_CAPTURING_REGEX,
    r"^ *(\d+) +(\d+) +Data +(\d+) +(\d+) +(\d+) *$"
);

pub fn check_version_line()->io::Result<String> {
    let output = execute_command_with_output(&[minfo_cli!(), "--version"])?;
    let line1 = output.lines().next();
    Ok(line1.map(String::from).unwrap_or_default())
}

fn minfo_string() -> io::Result<String> {
    let dev_arg = format!("dev={}", mutex_lock!(ARGS).drive.display());
    execute_command_with_output(&[
        minfo_cli!(),
        &dev_arg,
        "-minfo",
    ])
}

pub fn minfo_track_info() -> io::Result<Vec<Track>> {
    let output = minfo_string()?;
    let filtered = output
        .lines()
        .skip_while(|&x| !TRACKS_HEADER_REGEX.is_match(x))
        .skip(2)
        .take_while(|&x| !x.is_empty())
        .collect::<Vec<_>>();
    let mut tracks = Vec::new();
    for x in filtered {
        let _: Option<_> = try {
            let captures = TRACK_CAPTURING_REGEX.captures_iter(x).next()?;
            let track = Track {
                track_no: captures.get(1)?.as_str().parse().unwrap(), /* the RegExp asserts it's a `\d` */
                session_no: captures.get(2)?.as_str().parse().unwrap(),
                start_addr: captures.get(3)?.as_str().parse().unwrap(),
                end_addr: captures.get(4)?.as_str().parse().unwrap(),
                size: captures.get(5)?.as_str().parse().unwrap(),
            };
            tracks.push(track);
        };
    }
    Ok(tracks)
}

pub macro minfo_cli() {
    mutex_lock!(ARGS).minfo_program.name()
}

pub fn minfo_cli() -> String {
    minfo_cli!().into()
}
