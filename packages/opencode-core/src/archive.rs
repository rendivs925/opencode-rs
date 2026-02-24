use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use napi::Result;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use tar::Archive;
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
    match detect_format(archive_path) {
        ArchiveFormat::Zip => extract_zip(archive_path, output_dir),
        ArchiveFormat::Tar => extract_tar(File::open(archive_path).map_err(to_err)?, output_dir),
        ArchiveFormat::TarGz => extract_tar(GzDecoder::new(File::open(archive_path).map_err(to_err)?), output_dir),
        ArchiveFormat::TarBz2 => extract_tar(BzDecoder::new(File::open(archive_path).map_err(to_err)?), output_dir),
        ArchiveFormat::Unknown => Err(napi::Error::from_reason("Unsupported archive format")),
    }
}

#[napi]
pub fn list_archive_contents(archive_path: String) -> Result<Vec<ArchiveEntry>> {
    let archive_path = Path::new(&archive_path);
    match detect_format(archive_path) {
        ArchiveFormat::Zip => list_zip(archive_path),
        ArchiveFormat::Tar => list_tar(File::open(archive_path).map_err(to_err)?),
        ArchiveFormat::TarGz => list_tar(GzDecoder::new(File::open(archive_path).map_err(to_err)?)),
        ArchiveFormat::TarBz2 => list_tar(BzDecoder::new(File::open(archive_path).map_err(to_err)?)),
        ArchiveFormat::Unknown => Err(napi::Error::from_reason("Unsupported archive format")),
    }
}

#[napi]
pub fn read_archive_entry(archive_path: String, entry_name: String) -> Result<Vec<u8>> {
    let archive_path = Path::new(&archive_path);
    match detect_format(archive_path) {
        ArchiveFormat::Zip => read_zip_entry(archive_path, &entry_name),
        ArchiveFormat::Tar => read_tar_entry(File::open(archive_path).map_err(to_err)?, &entry_name),
        ArchiveFormat::TarGz => read_tar_entry(
            GzDecoder::new(File::open(archive_path).map_err(to_err)?),
            &entry_name,
        ),
        ArchiveFormat::TarBz2 => read_tar_entry(
            BzDecoder::new(File::open(archive_path).map_err(to_err)?),
            &entry_name,
        ),
        ArchiveFormat::Unknown => Err(napi::Error::from_reason("Unsupported archive format")),
    }
}

#[napi]
pub fn extract_tar_gz(archive_path: String, output_dir: String) -> Result<Vec<ArchiveEntry>> {
    let file = File::open(&archive_path).map_err(to_err)?;
    extract_tar(GzDecoder::new(file), Path::new(&output_dir))
}

#[napi]
pub fn extract_tar_bz2(archive_path: String, output_dir: String) -> Result<Vec<ArchiveEntry>> {
    let file = File::open(&archive_path).map_err(to_err)?;
    extract_tar(BzDecoder::new(file), Path::new(&output_dir))
}

fn to_err(error: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(error.to_string())
}

enum ArchiveFormat {
    Zip,
    Tar,
    TarGz,
    TarBz2,
    Unknown,
}

fn detect_format(path: &Path) -> ArchiveFormat {
    let lower = path.to_string_lossy().to_ascii_lowercase();
    if lower.ends_with(".zip") {
        return ArchiveFormat::Zip;
    }
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        return ArchiveFormat::TarGz;
    }
    if lower.ends_with(".tar.bz2") || lower.ends_with(".tbz2") {
        return ArchiveFormat::TarBz2;
    }
    if lower.ends_with(".tar") {
        return ArchiveFormat::Tar;
    }
    ArchiveFormat::Unknown
}

fn extract_zip(archive_path: &Path, output_dir: &Path) -> Result<Vec<ArchiveEntry>> {
    std::fs::create_dir_all(output_dir).map_err(to_err)?;
    let file = File::open(archive_path).map_err(to_err)?;
    let mut archive = ZipArchive::new(file).map_err(to_err)?;
    let mut entries = Vec::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(to_err)?;
        let outpath = output_dir.join(file.name());
        entries.push(ArchiveEntry {
            name: file.name().to_string(),
            size: file.size().to_string(),
            is_dir: file.is_dir(),
        });
        if file.is_dir() {
            std::fs::create_dir_all(&outpath).map_err(to_err)?;
        } else {
            if let Some(p) = outpath.parent() {
                std::fs::create_dir_all(p).map_err(to_err)?;
            }
            let mut outfile = File::create(&outpath).map_err(to_err)?;
            std::io::copy(&mut file, &mut outfile).map_err(to_err)?;
        }
    }
    Ok(entries)
}

fn list_zip(archive_path: &Path) -> Result<Vec<ArchiveEntry>> {
    let file = File::open(archive_path).map_err(to_err)?;
    let mut archive = ZipArchive::new(file).map_err(to_err)?;
    Ok((0..archive.len())
        .filter_map(|i| {
            let file = archive.by_index_raw(i).ok()?;
            Some(ArchiveEntry {
                name: file.name().to_string(),
                size: file.size().to_string(),
                is_dir: file.is_dir(),
            })
        })
        .collect())
}

fn read_zip_entry(archive_path: &Path, entry_name: &str) -> Result<Vec<u8>> {
    let file = File::open(archive_path).map_err(to_err)?;
    let mut archive = ZipArchive::new(file).map_err(to_err)?;
    let mut file = archive.by_name(entry_name).map_err(to_err)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).map_err(to_err)?;
    Ok(buffer)
}

fn extract_tar<R: Read>(reader: R, output_dir: &Path) -> Result<Vec<ArchiveEntry>> {
    std::fs::create_dir_all(output_dir).map_err(to_err)?;
    let mut archive = Archive::new(reader);
    let mut entries = Vec::new();
    for entry in archive.entries().map_err(to_err)? {
        let mut entry = entry.map_err(to_err)?;
        let path = entry.path().map_err(to_err)?.into_owned();
        let outpath = output_dir.join(&path);
        let is_dir = entry.header().entry_type().is_dir();
        entries.push(ArchiveEntry {
            name: path.to_string_lossy().to_string(),
            size: entry.size().to_string(),
            is_dir,
        });
        if is_dir {
            std::fs::create_dir_all(&outpath).map_err(to_err)?;
            continue;
        }
        if let Some(parent) = outpath.parent() {
            std::fs::create_dir_all(parent).map_err(to_err)?;
        }
        entry.unpack(&outpath).map_err(to_err)?;
    }
    Ok(entries)
}

fn list_tar<R: Read>(reader: R) -> Result<Vec<ArchiveEntry>> {
    let mut archive = Archive::new(reader);
    let mut entries = Vec::new();
    for entry in archive.entries().map_err(to_err)? {
        let entry = entry.map_err(to_err)?;
        let path = entry.path().map_err(to_err)?.into_owned();
        let is_dir = entry.header().entry_type().is_dir();
        entries.push(ArchiveEntry {
            name: path.to_string_lossy().to_string(),
            size: entry.size().to_string(),
            is_dir,
        });
    }
    Ok(entries)
}

fn read_tar_entry<R: Read>(reader: R, entry_name: &str) -> Result<Vec<u8>> {
    let mut archive = Archive::new(reader);
    for entry in archive.entries().map_err(to_err)? {
        let mut entry = entry.map_err(to_err)?;
        let path = entry.path().map_err(to_err)?.into_owned();
        if path.to_string_lossy() != entry_name {
            continue;
        }
        let mut buffer = Vec::new();
        entry.read_to_end(&mut buffer).map_err(to_err)?;
        return Ok(buffer);
    }
    Err(napi::Error::from_reason(format!(
        "Archive entry not found: {entry_name}"
    )))
}
