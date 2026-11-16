//! The system clipboard. On Linux its reads run on a worker thread and wait a bounded time.
//! The bounded reads are adapted from VectorCraft `apps/vectorcraft/src/clipboard.rs` at
//! 825620eb6cd17a4298b4fa9041013abe3d35190c (MIT OR Apache-2.0).

#[cfg(any(target_os = "linux", test))]
use std::sync::mpsc;
#[cfg(any(target_os = "linux", test))]
use std::time::{Duration, Instant};

/// Copies go out through the UI thread's arboard handle. On Linux reads go through a worker with
/// a handle of its own and wait [`LOAD_WAIT`] at most, a bitmap [`IMAGE_WAIT`]: arboard's Wayland
/// data-control read waits for the app that copied with no limit, and its X11 read up to 4 s for
/// each format it tries, which froze the window.
#[derive(Default)]
pub(super) struct NativeClipboard {
    /// Dropped before `handle`, after waiting for the worker's handle to go: on X11 the last handle
    /// dropped hands what the app copied to the clipboard manager, and it must be the app's own.
    #[cfg(target_os = "linux")]
    reads: Option<Loads<Read, Answer>>,
    handle: Option<arboard::Clipboard>,
}

impl NativeClipboard {
    pub(super) fn writer(&mut self) -> Result<&mut arboard::Clipboard, String> {
        match self.handle {
            Some(ref mut handle) => Ok(handle),
            None => Ok(self.handle.insert(arboard::Clipboard::new().map_err(|e| e.to_string())?)),
        }
    }

    pub(super) fn image(&mut self) -> Option<(u32, u32, Vec<u8>)> {
        #[cfg(target_os = "linux")]
        return match self.read(Read::Image, IMAGE_WAIT)? {
            Answer::Image(image) => image,
            Answer::Files(_) => None,
        };
        #[cfg(not(target_os = "linux"))]
        read_image(self.writer().ok()?)
    }

    pub(super) fn files(&mut self) -> Vec<std::path::PathBuf> {
        #[cfg(target_os = "linux")]
        return match self.read(Read::Files, LOAD_WAIT) {
            Some(Answer::Files(files)) => files,
            _ => vec![],
        };
        #[cfg(not(target_os = "linux"))]
        self.writer().ok().and_then(|cb| cb.get().file_list().ok()).unwrap_or_default()
    }

    #[cfg(target_os = "linux")]
    fn read(&mut self, request: Read, wait: Duration) -> Option<Answer> {
        if self.reads.is_none() {
            let mut reader: Option<arboard::Clipboard> = None;
            self.reads = Loads::spawn("clipboard-read", finish(on_data_control()), move |request| {
                let cb = match reader {
                    Some(ref mut cb) => cb,
                    None => match arboard::Clipboard::new() {
                        Ok(cb) => reader.insert(cb),
                        Err(_) => return Answer::empty(request),
                    },
                };
                match request {
                    Read::Image => Answer::Image(read_image(cb)),
                    Read::Files => Answer::Files(cb.get().file_list().unwrap_or_default()),
                }
            });
        }
        self.reads.as_mut()?.get(request, wait).ok()
    }
}

fn read_image(cb: &mut arboard::Clipboard) -> Option<(u32, u32, Vec<u8>)> {
    super::guarded_clipboard(|| {
        if let Ok(image) = cb.get_image() {
            return Some((image.width as u32, image.height as u32, image.bytes.into_owned()));
        }
        #[cfg(target_os = "linux")]
        return wayland_clipboard_image();
        #[cfg(not(target_os = "linux"))]
        None
    })
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy)]
enum Read {
    Image,
    Files,
}

#[cfg(target_os = "linux")]
enum Answer {
    Image(Option<(u32, u32, Vec<u8>)>),
    Files(Vec<std::path::PathBuf>),
}

#[cfg(target_os = "linux")]
impl Answer {
    fn empty(request: Read) -> Self {
        match request {
            Read::Image => Self::Image(None),
            Read::Files => Self::Files(vec![]),
        }
    }
}

/// The longest a bitmap read waits: the transfer and the decoding happen within it.
#[cfg(target_os = "linux")]
const IMAGE_WAIT: Duration = Duration::from_secs(10);

/// The longest an X11 read of arboard's lasts while the owner sends nothing, with room to spare:
/// arboard waits up to 4 s for each format it tries, and the worker's image and file-list reads
/// try one each.
#[cfg(target_os = "linux")]
const X11_READ_LIMIT: Duration = Duration::from_secs(25);

/// Whether arboard uses the compositor's data-control protocol rather than X11, by the check
/// arboard makes when it opens.
#[cfg(target_os = "linux")]
fn on_data_control() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some() && wl_clipboard_rs::utils::is_primary_selection_supported().is_ok()
}

/// How long after an unanswered read was sent dropping the reads waits for it to end: on X11 the
/// clipboard manager handoff needs the worker's handle gone; on data-control a read may never end.
#[cfg(target_os = "linux")]
fn finish(data_control: bool) -> Duration {
    if data_control { Duration::ZERO } else { X11_READ_LIMIT }
}

/// The longest a read waits for the clipboard's owner to send its data.
#[cfg(any(target_os = "linux", test))]
const LOAD_WAIT: Duration = Duration::from_secs(2);
/// What a read says once its worker has stopped.
#[cfg(any(target_os = "linux", test))]
const GONE: &str = "the clipboard's reader stopped";

/// Requests of type `R` answered with `A` on a worker thread.
#[cfg(any(target_os = "linux", test))]
struct Loads<R, A> {
    requests: mpsc::Sender<R>,
    answers: mpsc::Receiver<A>,
    /// When a read that timed out and hasn't answered yet was sent: until it answers, reads don't
    /// queue behind it.
    pending: Option<Instant>,
    /// How long after it was sent dropping `Loads` waits for an unanswered read to end.
    finish: Duration,
}

/// When dropping `Loads` stops waiting for its worker: `finish` after the unanswered read
/// `pending` was sent, and with no read unanswered [`LOAD_WAIT`] from `now` at most (an idle
/// worker ends at once).
#[cfg(any(target_os = "linux", test))]
fn stop_by(pending: Option<Instant>, now: Instant, finish: Duration) -> Instant {
    pending.map_or(now + LOAD_WAIT, |sent| sent + finish)
}

#[cfg(any(target_os = "linux", test))]
impl<R: Send + 'static, A: Send + 'static> Loads<R, A> {
    /// A worker thread named `name` that answers each request with `load`. None when it can't start.
    fn spawn(name: &str, finish: Duration, mut load: impl FnMut(R) -> A + Send + 'static) -> Option<Self> {
        let (requests, asked) = mpsc::channel::<R>();
        let (answer, answers) = mpsc::channel();
        let work = move || {
            while let Ok(request) = asked.recv() {
                if answer.send(load(request)).is_err() {
                    break;
                }
            }
            // `load` and the clipboard it holds go first: dropping `Loads` waits for `answer`.
            drop(load);
            drop(answer);
        };
        std::thread::Builder::new().name(name.into()).spawn(work).ok()?;
        Some(Self { requests, answers, pending: None, finish })
    }

    /// The answer to `request`, or why there is none, after `wait` at most.
    fn get(&mut self, request: R, wait: Duration) -> Result<A, String> {
        if self.pending.is_some() {
            match self.answers.try_recv() {
                // The late answer of the read that timed out: stale by now.
                Ok(_) => self.pending = None,
                Err(mpsc::TryRecvError::Empty) => return Err("the clipboard's owner hasn't answered an earlier read".into()),
                Err(mpsc::TryRecvError::Disconnected) => return Err(GONE.into()),
            }
        }
        let sent = Instant::now();
        self.requests.send(request).map_err(|_| GONE.to_string())?;
        match self.answers.recv_timeout(wait) {
            Ok(answer) => Ok(answer),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.pending = Some(sent);
                Err(format!("the clipboard's owner didn't send its data within {wait:?}"))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(GONE.into()),
        }
    }
}

#[cfg(any(target_os = "linux", test))]
impl<R, A> Drop for Loads<R, A> {
    /// Stops the worker and waits until it has let go of what its `load` holds, until [`stop_by`].
    fn drop(&mut self) {
        let (stop, _) = mpsc::channel();
        drop(std::mem::replace(&mut self.requests, stop));
        let end = stop_by(self.pending, Instant::now(), self.finish);
        while self.answers.recv_timeout(end.saturating_duration_since(Instant::now())).is_ok() {}
    }
}

#[cfg(target_os = "linux")]
fn wayland_clipboard_image() -> Option<(u32, u32, Vec<u8>)> {
    use std::io::Read;
    use wl_clipboard_rs::paste::{ClipboardType, MimeType, Seat, get_contents, get_mime_types_ordered};
    const MAX_BYTES: u64 = 4 * 16384 * 16384;
    let offers = get_mime_types_ordered(ClipboardType::Regular, Seat::Unspecified).ok()?;
    image_from_offers(&offers, |mime| {
        let (pipe, _) = get_contents(ClipboardType::Regular, Seat::Unspecified, MimeType::Specific(mime)).ok()?;
        let mut bytes = Vec::new();
        pipe.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
        (bytes.len() as u64 <= MAX_BYTES).then_some(bytes)
    })
}

#[cfg(any(target_os = "linux", test))]
fn image_from_offers(offers: &[String], mut fetch: impl FnMut(&str) -> Option<Vec<u8>>) -> Option<(u32, u32, Vec<u8>)> {
    offers.iter().filter(|mime| mime.starts_with("image/")).find_map(|mime| {
        let bytes = fetch(mime)?;
        // TGA has no magic number, so its MIME type must also identify it.
        if photocraft_codecs::detect(&bytes)? == photocraft_codecs::Format::Tga && !mime.contains("tga") {
            return None;
        }
        let image = photocraft_codecs::decode(&bytes).ok()?;
        Some((image.width(), image.height(), image.to_rgba8()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_codecs::{ChannelLayout, EncodeOptions, Image};

    #[test]
    fn clipboard_images_decode_non_png_offers_without_fetching_text() {
        for (mime, format) in
            [("image/jpeg", photocraft_codecs::Format::Jpeg), ("image/bmp", photocraft_codecs::Format::Bmp), ("image/webp", photocraft_codecs::Format::WebP)]
        {
            let image = Image::from_u8(3, 2, ChannelLayout::Rgba, [255, 0, 0, 255].repeat(6)).unwrap();
            let encoded = photocraft_codecs::encode(&image, format, &EncodeOptions::default()).unwrap();
            let offers = ["text/plain".to_string(), "image/png".to_string(), mime.to_string()];
            let result = image_from_offers(&offers, |offered| {
                assert_ne!(offered, "text/plain");
                Some(if offered == mime { encoded.clone() } else { vec![0; 20] })
            })
            .unwrap();
            assert_eq!((result.0, result.1), (3, 2));
            assert_eq!(result.2.len(), 24);
            if format != photocraft_codecs::Format::Jpeg {
                assert_eq!(result.2, [255, 0, 0, 255].repeat(6));
            }
        }
    }

    #[test]
    fn clipboard_tga_requires_its_mime_type() {
        assert_eq!(image_from_offers(&["image/x-tga".into()], |_| Some(crate::services::tests::tga_1x1())), Some((1, 1, vec![255, 0, 0, 255])));
        assert!(image_from_offers(&["image/unknown".into()], |_| Some(crate::services::tests::tga_1x1())).is_none());
        assert!(image_from_offers(&["image/png".into()], |_| Some(vec![0; 8])).is_none());
        assert!(image_from_offers(&[], |_| panic!("no offers to fetch")).is_none());
    }

    #[test]
    fn a_read_that_answers_in_time_gives_the_answer() {
        let mut loads = Loads::spawn("test", Duration::ZERO, |()| "image".to_string()).unwrap();
        assert_eq!(loads.get((), Duration::from_secs(5)), Ok("image".to_string()));
        assert_eq!(loads.get((), Duration::from_secs(5)), Ok("image".to_string()));
    }

    #[test]
    fn an_owner_that_never_answers_holds_the_worker_not_the_caller() {
        let (release, gate) = mpsc::channel::<()>();
        let mut calls = 0;
        let mut loads = Loads::spawn("test", Duration::ZERO, move |()| {
            calls += 1;
            if calls == 1 {
                let _ = gate.recv();
                return "stale".to_string();
            }
            "fresh".to_string()
        })
        .unwrap();
        assert!(loads.get((), Duration::ZERO).is_err(), "the first read times out");
        // While it is unanswered, a read doesn't queue behind it or wait at all.
        assert!(loads.get((), Duration::from_secs(600)).is_err());
        release.send(()).unwrap();
        // Once the late answer is in, it is dropped and a new read is made.
        let answer = loop {
            match loads.get((), Duration::from_secs(600)) {
                Ok(answer) => break answer,
                Err(why) => {
                    assert_ne!(why, GONE);
                    std::thread::yield_now();
                }
            }
        };
        assert_eq!(answer, "fresh");
    }

    #[test]
    fn dropping_the_reads_waits_until_the_worker_lets_go_of_its_clipboard() {
        let clipboard = std::sync::Arc::new(());
        let held = clipboard.clone();
        let mut loads = Loads::spawn("test", Duration::ZERO, move |()| std::sync::Arc::strong_count(&held)).unwrap();
        assert_eq!(loads.get((), Duration::from_secs(600)), Ok(2));
        drop(loads);
        assert_eq!(std::sync::Arc::strong_count(&clipboard), 1, "the worker's handle is gone before the app's");
    }

    #[test]
    fn dropping_the_reads_waits_for_an_unanswered_read_up_to_its_limit() {
        let now = Instant::now();
        let limit = Duration::from_secs(25);
        assert_eq!(stop_by(None, now, limit), now + LOAD_WAIT);
        assert_eq!(stop_by(Some(now - Duration::from_secs(2)), now, limit), now + Duration::from_secs(23));
        assert!(stop_by(Some(now - Duration::from_secs(30)), now, limit) <= now);
        assert_eq!(stop_by(Some(now), now, Duration::ZERO), now);
    }
}
