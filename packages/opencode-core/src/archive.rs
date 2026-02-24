use flate2::read::GzDecoder;
use napi::Result;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

#[napi]
pub struct ArchiveEntry {
    pub name: String,
    pub size: String,
    pub is_dir: bool,
}

#[napi]
pub fn extract_archive(archive_path: String, output_dir: String) -> Result<Vec<ArchiveEntry>> {
    let archive_path = Path::new(&archive_path);
    let output_dir = Path::new(&output_dir);

    std::fs::create_dir_all(output_dir).map_err(|e| napi::Error::from_reason(e.to_string()))?;

    let file = File::open(archive_path).map_err(|e| napi::Error::from_reason(e.to_string()))?;
    let mut archive = ZipArchive::new(file).map_err(|e| napi::Error::from_reason(e.to_string()))?;

    let mut entries = Vec::new();

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let outpath = output_dir.join(file.name());

        let entry = ArchiveEntry {
            name: file.name().to_string(),
            size: file.size().to_string(),
            is_dir: file.is_dir(),
        };

        if file.is_dir() {
            std::fs::create_dir_all(&outpath)
                .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        } else {
            if let Some(p) = outpath.parent() {
                std::fs::create_dir_all(p).map_err(|e| napi::Error::from_reason(e.to_string()))?;
            }
            let mut outfile =
                File::create(&outpath).map_err(|e| napi::Error::from_reason(e.to_string()))?;
            std::io::copy(&mut file, &mut outfile)
                .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        }

        entries.push(entry);
    }

    Ok(entries)
}

#[napi]
pub fn list_archive_contents(archive_path: String) -> Result<Vec<ArchiveEntry>> {
    let file = File::open(&archive_path).map_err(|e| napi::Error::from_reason(e.to_string()))?;
    let mut archive = ZipArchive::new(file).map_err(|e| napi::Error::from_reason(e.to_string()))?;

    let entries: Vec<ArchiveEntry> = (0..archive.len())
        .filter_map(|i| {
            let file = archive.by_index_raw(i).ok()?;
            Some(ArchiveEntry {
                name: file.name().to_string(),
                size: file.size().to_string(),
                is_dir: file.is_dir(),
            })
        })
        .collect();

    Ok(entries)
}

#[napi]
pub fn read_archive_entry(archive_path: String, entry_name: String) -> Result<Vec<u8>> {
    let file = File::open(&archive_path).map_err(|e| napi::Error::from_reason(e.to_string()))?;
    let mut archive = ZipArchive::new(file).map_err(|e| napi::Error::from_reason(e.to_string()))?;

    let mut file = archive
        .by_name(&entry_name)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;

    Ok(buffer)
}

#[napi]
pub fn extract_tar_gz(archive_path: String, output_dir: String) -> Result<Vec<ArchiveEntry>> {
    let file = File::open(&archive_path).map_err(|e| napi::Error::from_reason(e.to_string()))?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    std::fs::create_dir_all(&output_dir).map_err(|e| napi::Error::from_reason(e.to_string()))?;

    let mut entries = Vec::new();

    for entry in archive
        .entries()
        .map_err(|e| napi::Error::from_reason(e.to_string()))?
    {
        let mut entry = entry.map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let path = entry
            .path()
            .map_err(|e| napi::Error::from_reason(e.to_string()))?
            .into_owned();
        let outpath = Path::new(&output_dir).join(&path);

        let is_dir = entry.header().entry_type().is_dir();

        entries.push(ArchiveEntry {
            name: path.to_string_lossy().to_string(),
            size: entry.size().to_string(),
            is_dir,
        });

        if is_dir {
            std::fs::create_dir_all(&outpath)
                .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        } else {
            if let Some(p) = outpath.parent() {
                std::fs::create_dir_all(p).map_err(|e| napi::Error::from_reason(e.to_string()))?;
            }
            entry
                .unpack(&outpath)
                .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        }
    }

    Ok(entries)
}
