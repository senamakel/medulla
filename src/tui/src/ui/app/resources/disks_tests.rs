//! Unit tests for `disks`.

use super::disk_name;

#[test]
fn disk_names_follow_btop() {
    assert_eq!(disk_name("/"), "root");
    assert_eq!(disk_name("/System/Volumes/Data"), "Data");
    assert_eq!(disk_name("/mnt/backup/"), "backup");
    assert_eq!(disk_name("C:\\"), "C:\\");
}
