use webring_core::vfs::tar::TarArchive;
use webring_core::vfs::{Vfs, AT_FDCWD};

fn create_test_tar() -> Vec<u8> {
    let mut tar = Vec::new();

    // Helper to append a 512-byte tar header + data blocks
    let mut add_entry = |name: &str, content: &[u8], is_dir: bool| {
        let mut header = [0u8; 512];
        let name_bytes = name.as_bytes();
        header[..name_bytes.len()].copy_from_slice(name_bytes);

        // mode: 0644 for file, 0755 for dir
        let mode = if is_dir { b"0000755\0" } else { b"0000644\0" };
        header[100..108].copy_from_slice(mode);

        // size in octal
        let size_str = format!("{:011o}\0", content.len());
        header[124..136].copy_from_slice(size_str.as_bytes());

        // typeflag: '5' for dir, '0' for file
        header[156] = if is_dir { b'5' } else { b'0' };

        // magic: ustar\0
        header[257..263].copy_from_slice(b"ustar\0");

        tar.extend_from_slice(&header);

        if !is_dir && !content.is_empty() {
            tar.extend_from_slice(content);
            let pad = (content.len() + 511) / 512 * 512 - content.len();
            tar.extend_from_slice(&vec![0u8; pad]);
        }
    };

    add_entry("etc/config.json", b"{\"name\": \"webring-container\"}", false);
    add_entry("var/log/", b"", true);
    add_entry("var/log/app.log", b"Container log line 1\n", false);

    // End of archive: two 512-byte zero blocks
    tar.extend_from_slice(&[0u8; 1024]);
    tar
}

#[test]
fn test_tar_archive_parser() {
    let tar_bytes = create_test_tar();
    let entries = TarArchive::parse(&tar_bytes).unwrap();

    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].path, "/etc/config.json");
    assert_eq!(entries[0].is_dir, false);
    assert_eq!(entries[0].data, b"{\"name\": \"webring-container\"}");

    assert_eq!(entries[1].path, "/var/log");
    assert_eq!(entries[1].is_dir, true);

    assert_eq!(entries[2].path, "/var/log/app.log");
    assert_eq!(entries[2].data, b"Container log line 1\n");
}

#[test]
fn test_vfs_mount_tar_and_file_operations() {
    let mut vfs = Vfs::new();
    let tar_bytes = create_test_tar();

    let count = vfs.mount_tar(&tar_bytes).unwrap();
    assert_eq!(count, 3);

    // Read the mounted file
    let fd = vfs.openat(AT_FDCWD, "/etc/config.json", 0, 0);
    assert!(fd >= 3);

    let mut buf = [0u8; 64];
    let n = vfs.read(fd as i32, &mut buf);
    assert_eq!(n, 29);
    assert_eq!(&buf[..n as usize], b"{\"name\": \"webring-container\"}");

    vfs.close(fd as i32);

    // Read directory entries (getdents64) on /var
    let dir_fd = vfs.openat(AT_FDCWD, "/var", 0, 0);
    assert!(dir_fd >= 3);

    let mut dents_buf = [0u8; 512];
    let read_dents = vfs.getdents64(dir_fd as i32, &mut dents_buf);
    assert!(read_dents > 0);

    vfs.close(dir_fd as i32);
}
