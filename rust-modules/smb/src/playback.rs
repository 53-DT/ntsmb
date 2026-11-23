//! Prepare an SMB file for the existing native player without involving a media server.

use crate::{range_http::RangeServer, Session};

/// A prepared direct-play source. Keep the bridge alive until playback has stopped.
pub struct PreparedPlayback {
    pub play: plx_playback_contract::playurl::PlayUrl,
    pub bridge: RangeServer,
}

/// Open one share-relative SMB file, expose it through loopback HTTP Range and inspect the actual
/// codecs with the bundled FFmpeg demuxer. The authenticated session is transferred into RangeServer and disconnected
/// only after the file handle and any in-flight HTTP range handlers have been released.
pub fn prepare_playback(mut session: Session, path: &str) -> Result<PreparedPlayback, String> {
    let reader = session.open_file_blocking(path)?;
    let bridge = RangeServer::start_with_session(reader, session)?;
    let probe = plx_play_feed::probe_url(bridge.url())?;
    let video_codec = probe
        .video_codec
        .ok_or_else(|| "No video stream found".to_string())?;
    let (profile, compatibility, enhancement) = probe
        .dovi
        .map(|(profile, compatibility, enhancement)| {
            (
                i64::from(profile),
                i64::from(compatibility),
                enhancement,
            )
        })
        .unwrap_or((0, 0, false));

    let play = plx_playback_contract::playurl::PlayUrl {
        url: bridge.url().to_owned(),
        vcodec: video_codec,
        acodec: probe.audio_codec.unwrap_or_default(),
        fps: 0.0,
        dovi: plx_playback_contract::playurl::PlayDovi {
            profile,
            bl_compat: compatibility,
            el_present: enhancement,
        },
        atmos: false,
        auto_source_kbps: 0,
        auto_hls_base: String::new(),
        auto_start_hls: false,
        source_raster: (probe.width > 0 && probe.height > 0)
            .then_some([probe.width, probe.height]),
    };

    Ok(PreparedPlayback { play, bridge })
}
