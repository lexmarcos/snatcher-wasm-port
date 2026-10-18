//! Native helpers for the importer: image files from the file system and a directory sink (tests, `snt import-disc`).
use super::{DiscIo, Sink};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// The image files of a cue sheet read with `std::fs`. `dir` = directory of the cue; names are looked up case-insensitively there.
pub struct FsIo { files: Vec<File> }

impl FsIo {
    /// Open every `FILE` of `cue` (relative to `dir`); returns the reader and the file sizes.
    pub fn open(dir: &Path, names: &[String]) -> Result<(FsIo, Vec<u64>), String> {
        let mut files = Vec::new();
        let mut sizes = Vec::new();
        for n in names {
            let direct = dir.join(n);
            let path = if direct.exists() { direct } else {
                fs::read_dir(dir).map_err(|e| e.to_string())?.filter_map(|e| e.ok())
                    .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(n)).map(|e| e.path())
                    .ok_or_else(|| format!("{n}: not found next to the cue sheet"))?
            };
            let f = File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            sizes.push(f.metadata().map_err(|e| e.to_string())?.len());
            files.push(f);
        }
        Ok((FsIo { files }, sizes))
    }
}

impl DiscIo for FsIo {
    fn read(&mut self, file: usize, offset: u64, buf: &mut [u8]) -> Result<(), String> {
        let f = self.files.get_mut(file).ok_or("no such file")?;
        f.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
        f.read_exact(buf).map_err(|e| format!("read {} bytes at {offset}: {e}", buf.len()))
    }
}

/// Writes every stream to a file below `root` (`files/NAME` creates the directory).
pub struct DirSink { pub root: PathBuf, open: HashMap<String, File>, pub log: Vec<(String, u64, u64)> }

impl DirSink {
    pub fn new(root: &Path) -> DirSink { DirSink { root: root.to_path_buf(), open: HashMap::new(), log: Vec::new() } }
}

impl Sink for DirSink {
    fn write(&mut self, stream: &str, data: &[u8]) -> Result<(), String> {
        if !self.open.contains_key(stream) {
            let p = self.root.join(stream);
            if let Some(d) = p.parent() { fs::create_dir_all(d).map_err(|e| e.to_string())?; }
            self.open.insert(stream.to_string(), File::create(&p).map_err(|e| format!("{}: {e}", p.display()))?);
        }
        self.open.get_mut(stream).unwrap().write_all(data).map_err(|e| e.to_string())
    }
    fn finish(&mut self, stream: &str) -> Result<(), String> {
        if !self.open.contains_key(stream) { self.write(stream, &[])?; }
        self.open.remove(stream);
        Ok(())
    }
    fn progress(&mut self, phase: &str, done: u64, total: u64) {
        if self.log.last().map(|l| l.0 != phase).unwrap_or(true) { self.log.push((phase.to_string(), done, total)); }
    }
}
