//! Prepare an SMB file for the existing native player without involving a media server.

use crate::{range_http::RangeServer, Session};

/// A prepared direct-play source. Keep the bridge alive until playback has stopped.
pub struct PreparedPlayback {
    pub play: plx_playback_contract::playurl::PlayUrl,
    pub bridge: RangeServer,
    /// Probe facts retained for the local playback metadata/track menu.
    pub probe: plx_play_feed::SourceProbe,
}

/// Open one share-relative SMB file, expose it through loopback HTTP Range and inspect the actual
/// codecs with the bundled FFmpeg demuxer. The authenticated session is transferred into RangeServer and disconnected
/// only after the file handle and any in-flight HTTP range handlers have been released.
struct SmbProbeSource {
    reader: std::sync::Arc<smb2::FileReader>,
    size: u64,
}

impl plx_play_feed::ProbeSource for SmbProbeSource {
    fn size(&self) -> u64 { self.size }

    fn read_at(&self, offset: u64, length: usize) -> Result<Vec<u8>, String> {
        smol::block_on(self.reader.read_at(offset, length as u64))
            .map_err(|error| format!("SMB random read failed: {error}"))
    }
}

pub fn prepare_playback(mut session: Session, path: &str) -> Result<PreparedPlayback, String> {
    let reader = std::sync::Arc::new(session.open_file_blocking(path)?);
    let source = SmbProbeSource { size: reader.size(), reader: reader.clone() };
    let probe = plx_play_feed::probe_source(std::sync::Arc::new(source))
        .map_err(|error| format!("FFmpeg custom-AVIO probe failed: {error}"))?;
    let bridge = RangeServer::start_shared_with_session(reader, session)?;
    let video_codec = probe
        .video_codec
        .clone()
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
        acodec: probe.audio_codec.clone().unwrap_or_default(),
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

    Ok(PreparedPlayback { play, bridge, probe })
}
