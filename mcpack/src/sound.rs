//! Sound modes for the pack:
//! 1 = original, 2 = light (q0.1, max 32 kHz), 3 = heavy (mono 22.05 kHz, lowest quality),
//! 4 = silent placeholders with the same length (mono 8 kHz).

use std::io::Cursor;
use std::num::{NonZeroU32, NonZeroU8};
use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisDecoder, VorbisEncoderBuilder};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Original = 1,
    Light = 2,
    Heavy = 3,
    Silent = 4,
}

impl Mode {
    pub fn from_num(n: u32) -> Option<Mode> {
        Some(match n {
            1 => Mode::Original,
            2 => Mode::Light,
            3 => Mode::Heavy,
            4 => Mode::Silent,
            _ => return None,
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            Mode::Original => "original",
            Mode::Light => "light compression",
            Mode::Heavy => "heavy compression",
            Mode::Silent => "silent (same length)",
        }
    }
}

/// Re-encodes one Ogg Vorbis file. Returns None to keep the original (mode 1, or any failure).
pub fn convert(data: &[u8], mode: Mode) -> Option<Vec<u8>> {
    if mode == Mode::Original {
        return None;
    }
    let mut dec = VorbisDecoder::new(Cursor::new(data.to_vec())).ok()?;
    let src_rate = dec.sampling_frequency().get();
    let src_ch = dec.channels().get() as usize;
    let mut chans: Vec<Vec<f32>> = vec![Vec::new(); src_ch];
    while let Some(block) = dec.decode_audio_block().ok()? {
        for (c, s) in block.samples().iter().enumerate() {
            chans[c].extend_from_slice(s);
        }
    }
    let frames = chans.first().map_or(0, Vec::len);
    if frames == 0 {
        return None;
    }
    let seconds = frames as f64 / src_rate as f64;

    let (rate, out, quality) = match mode {
        Mode::Light => {
            let rate = src_rate.min(32000);
            (rate, chans.iter().map(|c| resample(c, src_rate, rate)).collect::<Vec<_>>(), 0.1)
        }
        Mode::Heavy => {
            let mono = downmix(&chans);
            let rate = src_rate.min(22050);
            (rate, vec![resample(&mono, src_rate, rate)], -0.1)
        }
        Mode::Silent => {
            let rate = 8000;
            let n = (seconds * rate as f64).round().max(1.0) as usize;
            (rate, vec![vec![0.0f32; n]], -0.1)
        }
        Mode::Original => unreachable!(),
    };

    let mut sink = Vec::new();
    let mut enc = VorbisEncoderBuilder::new_with_serial(
        NonZeroU32::new(rate)?,
        NonZeroU8::new(out.len() as u8)?,
        &mut sink,
        0x4d43,
    )
    .bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr { target_quality: quality })
    .build()
    .ok()?;
    for start in (0..out[0].len()).step_by(4096) {
        let end = (start + 4096).min(out[0].len());
        let block: Vec<&[f32]> = out.iter().map(|c| &c[start..end]).collect();
        enc.encode_audio_block(&block).ok()?;
    }
    enc.finish().ok()?;
    // Never make a file bigger than it was.
    (sink.len() < data.len()).then_some(sink)
}

fn downmix(chans: &[Vec<f32>]) -> Vec<f32> {
    if chans.len() == 1 {
        return chans[0].clone();
    }
    let n = chans[0].len();
    let k = 1.0 / chans.len() as f32;
    (0..n).map(|i| chans.iter().map(|c| c[i]).sum::<f32>() * k).collect()
}

/// Linear resampling; fine for game SFX at these rates.
fn resample(src: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || src.len() < 2 {
        return src.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let n = ((src.len() as f64) / ratio).round() as usize;
    (0..n)
        .map(|i| {
            let p = i as f64 * ratio;
            let j = p as usize;
            let t = (p - j as f64) as f32;
            let a = src[j.min(src.len() - 1)];
            let b = src[(j + 1).min(src.len() - 1)];
            a + (b - a) * t
        })
        .collect()
}
