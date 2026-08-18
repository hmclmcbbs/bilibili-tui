//! Unified image cache shared by every page that downloads covers, avatars and
//! product thumbnails.
//!
//! Layers, from fastest to slowest:
//!   1. in-memory decoded images (LRU by last use)
//!   2. on-disk raw bytes under `~/.cache/bilibili-tui/images/` (LRU by last use)
//!   3. network fetch (deduplicated: concurrent requests for the same URL share
//!      a single in-flight task)
//!
//! The disk cache has a byte cap and the memory cache has an entry cap; the
//! least-recently-used entries are evicted first, so frequently browsed covers
//! stay warm while one-off images get cleaned up automatically.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use image::DynamicImage;

const CACHE_SUBDIR: &str = "images";
const META_FILE: &str = "meta.json";
const DEFAULT_DISK_LIMIT_BYTES: u64 = 256 * 1024 * 1024; // 256 MiB
const DEFAULT_MEMORY_LIMIT: usize = 512;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn hash_key(url: &str) -> String {
    let digest = md5::compute(url.as_bytes());
    let bytes: &[u8] = digest.as_ref();
    let mut s = String::with_capacity(32);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct MetaEntry {
    url: String,
    last_used: u64,
    size: u64,
}

pub struct ImageCache {
    dir: PathBuf,
    meta_path: PathBuf,
    disk_limit_bytes: u64,
    memory_limit: usize,
    /// url -> (decoded image, last_used_secs)
    memory: Mutex<HashMap<String, (DynamicImage, u64)>>,
    /// url -> shared result holder for in-flight downloads
    inflight: Mutex<HashMap<String, Arc<tokio::sync::Mutex<Option<DynamicImage>>>>>,
    /// url -> disk metadata (last_used + size)
    meta: Mutex<HashMap<String, MetaEntry>>,
    /// monotonically increasing recency counter for memory LRU
    seq: AtomicU64,
    /// counter of pending meta changes; we batch disk meta writes instead of
    /// rewriting meta.json on every single image download (cheap under load).
    meta_dirty: AtomicU64,
}

static INSTANCE: OnceLock<ImageCache> = OnceLock::new();

/// Global cap on concurrent image downloads. Scrolling a dense grid can spawn
/// dozens of cover fetches at once; limiting concurrency keeps CDN pressure
/// and socket usage bounded while the LRU cache stays warm.
static DOWNLOAD_SEMAPHORE: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();

fn download_semaphore() -> &'static tokio::sync::Semaphore {
    DOWNLOAD_SEMAPHORE.get_or_init(|| Arc::new(tokio::sync::Semaphore::new(6)))
}

/// Get the process-wide image cache singleton.
pub fn instance() -> &'static ImageCache {
    INSTANCE.get_or_init(|| {
        let dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("bilibili-tui")
            .join(CACHE_SUBDIR);
        let meta_path = dir.join(META_FILE);
        let cache = ImageCache {
            dir,
            meta_path,
            disk_limit_bytes: DEFAULT_DISK_LIMIT_BYTES,
            memory_limit: DEFAULT_MEMORY_LIMIT,
            memory: Mutex::new(HashMap::new()),
            inflight: Mutex::new(HashMap::new()),
            meta: Mutex::new(HashMap::new()),
            seq: AtomicU64::new(0),
            meta_dirty: AtomicU64::new(0),
        };
        cache.load_meta();
        cache.evict_disk_if_needed();
        cache
    })
}

impl ImageCache {
    fn load_meta(&self) {
        let Ok(text) = std::fs::read_to_string(&self.meta_path) else {
            return;
        };
        let Ok(entries) = serde_json::from_str::<Vec<MetaEntry>>(&text) else {
            return;
        };
        let mut meta = self.meta.lock().unwrap();
        for e in entries {
            meta.insert(e.url.clone(), e);
        }
    }

    fn save_meta(&self) {
        let entries: Vec<MetaEntry> = self.meta.lock().unwrap().values().cloned().collect();
        if entries.is_empty() {
            return;
        }
        let _ = std::fs::create_dir_all(&self.dir);
        if let Ok(text) = serde_json::to_string(&entries) {
            let _ = std::fs::write(&self.meta_path, text);
        }
    }

    /// Record a meta change and flush to disk only after a batch accumulates.
    /// A single image fetch used to rewrite the whole meta.json synchronously;
    /// with dense grids that is needless file I/O, so we coalesce it.
    fn mark_meta_dirty(&self) {
        if self.meta_dirty.fetch_add(1, Ordering::Relaxed) + 1 >= 8 {
            self.save_meta();
            self.meta_dirty.store(0, Ordering::Relaxed);
        }
    }

    fn disk_path(&self, url: &str) -> PathBuf {
        self.dir.join(format!("{}.img", hash_key(url)))
    }

    fn touch_memory(&self, url: &str, img: DynamicImage) {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
        let mut mem = self.memory.lock().unwrap();
        mem.insert(url.to_string(), (img, seq));
        if mem.len() > self.memory_limit {
            // Evict the least-recently-used entry (scan; map is bounded and small).
            if let Some(oldest_url) = mem
                .iter()
                .min_by_key(|(_, (_, seq))| *seq)
                .map(|(u, _)| u.clone())
            {
                mem.remove(&oldest_url);
            }
        }
    }

    fn touch_disk(&self, url: &str) {
        let now = now_secs();
        if let Some(e) = self.meta.lock().unwrap().get_mut(url) {
            e.last_used = now;
        }
    }

    fn evict_disk_if_needed(&self) {
        let mut meta = self.meta.lock().unwrap();
        let mut total: u64 = meta.values().map(|e| e.size).sum();
        if total <= self.disk_limit_bytes {
            return;
        }
        // Remove oldest entries until under limit (keep at least one pass).
        let mut ordered: Vec<(String, u64)> = meta
            .iter()
            .map(|(url, e)| (url.clone(), e.last_used))
            .collect();
        ordered.sort_by_key(|(_, ts)| *ts);
        for (url, _) in ordered {
            if total <= self.disk_limit_bytes {
                break;
            }
            if let Some(e) = meta.remove(&url) {
                total = total.saturating_sub(e.size);
                let _ = std::fs::remove_file(self.disk_path(&url));
            }
        }
        drop(meta);
        self.save_meta();
    }

    /// Fetch a decoded image, consulting memory -> disk -> network in order.
    /// Returns `None` only when the network fetch or decode fails.
    pub async fn get(&self, url: &str) -> Option<DynamicImage> {
        // 1. memory
        {
            let mut mem = self.memory.lock().unwrap();
            if let Some((img, _)) = mem.get(url) {
                let img = img.clone();
                let seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
                mem.insert(url.to_string(), (img.clone(), seq));
                self.touch_disk(url);
                return Some(img);
            }
        }

        // 2. deduplicate concurrent fetches of the same URL
        let holder = {
            let mut inflight = self.inflight.lock().unwrap();
            if let Some(h) = inflight.get(url) {
                h.clone()
            } else {
                let h = Arc::new(tokio::sync::Mutex::new(None));
                inflight.insert(url.to_string(), h.clone());
                h
            }
        };

        // If someone else is already downloading, wait for their result.
        {
            let guard = holder.lock().await;
            if let Some(img) = guard.as_ref() {
                let img = img.clone();
                drop(guard);
                self.finish_fetch(url, img.clone());
                return Some(img);
            }
        }

        // 3. disk (raw bytes) -> decode
        let disk_bytes = tokio::fs::read(self.disk_path(url)).await.ok();
        if let Some(bytes) = disk_bytes {
            if let Some(img) = decode_image(bytes).await {
                let mut g = holder.lock().await;
                *g = Some(img.clone());
                self.touch_disk(url);
                drop(g);
                self.finish_fetch(url, img.clone());
                return Some(img);
            }
        }

        // 4. network
        let fetched = fetch_and_store(self, url).await;
        let mut g = holder.lock().await;
        *g = fetched.clone();
        drop(g);
        self.finish_fetch(url, fetched.clone()?);
        fetched
    }

    /// Remove the in-flight holder and insert into memory + disk LRU bookkeeping.
    fn finish_fetch(&self, url: &str, img: DynamicImage) {
        self.inflight.lock().unwrap().remove(url);
        self.touch_memory(url, img);
    }
}

async fn decode_image(bytes: Vec<u8>) -> Option<DynamicImage> {
    tokio::task::spawn_blocking(move || image::load_from_memory(&bytes).ok())
        .await
        .ok()
        .flatten()
}

async fn fetch_and_store(cache: &ImageCache, url: &str) -> Option<DynamicImage> {
    let _permit = download_semaphore().acquire().await.ok()?;
    let response = reqwest::get(url).await.ok()?;
    let bytes = response.bytes().await.ok()?;
    let img = decode_image(bytes.to_vec()).await?;
    // Store raw bytes on disk for next run.
    let path = cache.disk_path(url);
    let _ = tokio::fs::create_dir_all(cache.dir.clone()).await;
    let _ = tokio::fs::write(&path, &bytes).await;
    {
        let mut meta = cache.meta.lock().unwrap();
        meta.insert(
            url.to_string(),
            MetaEntry {
                url: url.to_string(),
                last_used: now_secs(),
                size: bytes.len() as u64,
            },
        );
    }
    cache.evict_disk_if_needed();
    cache.mark_meta_dirty();
    Some(img)
}

/// Synchronous peek into the memory cache (used by tests / non-async paths).
pub fn peek_memory(url: &str) -> Option<DynamicImage> {
    instance()
        .memory
        .lock()
        .unwrap()
        .get(url)
        .map(|(img, _)| img.clone())
}

/// Number of entries currently held in the disk LRU metadata.
pub fn disk_entry_count() -> usize {
    instance().meta.lock().unwrap().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_and_safe() {
        let a = hash_key("https://i0.hdslb.com/bfs/archive/abc.jpg");
        let b = hash_key("https://i0.hdslb.com/bfs/archive/abc.jpg");
        assert_eq!(a, b);
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        let c = hash_key("https://i0.hdslb.com/bfs/archive/def.jpg");
        assert_ne!(a, c);
    }

    #[test]
    fn memory_lru_evicts_oldest() {
        let cache = ImageCache {
            dir: PathBuf::from("/tmp/bilibili-tui-test-images"),
            meta_path: PathBuf::from("/tmp/bilibili-tui-test-images/meta.json"),
            disk_limit_bytes: DEFAULT_DISK_LIMIT_BYTES,
            memory_limit: 2,
            memory: Mutex::new(HashMap::new()),
            inflight: Mutex::new(HashMap::new()),
            meta: Mutex::new(HashMap::new()),
            seq: AtomicU64::new(1000),
            meta_dirty: AtomicU64::new(0),
        };
        let img = image::RgbaImage::new(4, 4);
        let dynimg = DynamicImage::ImageRgba8(img);
        // Insert entries with distinct timestamps (a oldest, c newest).
        cache.memory.lock().unwrap().insert(
            "a".to_string(),
            (dynimg.clone(), 1),
        );
        cache.memory.lock().unwrap().insert(
            "b".to_string(),
            (dynimg.clone(), 2),
        );
        cache.memory.lock().unwrap().insert("c".to_string(), (dynimg, 3));
        assert!(cache.memory.lock().unwrap().contains_key("a"));
        assert!(cache.memory.lock().unwrap().contains_key("b"));
        assert!(cache.memory.lock().unwrap().contains_key("c"));
        // Re-touch "a" so it becomes most recent, then inserting one more
        // entry evicts the oldest ("b").
        cache.touch_memory("a", DynamicImage::ImageRgba8(image::RgbaImage::new(4, 4)));
        cache.touch_memory("d", DynamicImage::ImageRgba8(image::RgbaImage::new(4, 4)));
        assert!(!cache.memory.lock().unwrap().contains_key("b"));
        assert!(cache.memory.lock().unwrap().contains_key("a"));
        assert!(cache.memory.lock().unwrap().contains_key("d"));
        // "c" was the next-oldest after "b" and got evicted on the second insert.
        assert!(!cache.memory.lock().unwrap().contains_key("c"));
    }
}
