//! Every local mount on macOS, read from the kernel's mount table.
//!
//! sysinfo lists only the volumes Finder would browse, which on a stock Mac is
//! the root and its Data twin. btop lists the rest of the APFS family too —
//! VM, Preboot, Update, xarts — and so does this.

/// One mounted local filesystem.
pub(super) struct Mount {
    /// The device node, `/dev/disk3s1s1`.
    pub device: String,
    pub mount: String,
    pub filesystem: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

/// Filesystems the kernel reports as local that hold no data of their own.
const PSEUDO: [&str; 3] = ["devfs", "autofs", "nullfs"];

/// The local mounts, in the kernel's mount order.
pub(super) fn local_mounts() -> Vec<Mount> {
    // SAFETY: a null buffer asks only for the count.
    let count = unsafe { libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT) };
    if count < 1 {
        return Vec::new();
    }
    let mut table: Vec<libc::statfs> = Vec::with_capacity(count as usize);
    let size = (count as usize * std::mem::size_of::<libc::statfs>()) as libc::c_int;
    // SAFETY: `table` has room for `count` entries, which is what `size` says.
    let filled = unsafe { libc::getfsstat(table.as_mut_ptr(), size, libc::MNT_NOWAIT) };
    if filled < 1 {
        return Vec::new();
    }
    // SAFETY: the kernel initialised the first `filled` entries, at most `count`.
    unsafe { table.set_len((filled as usize).min(count as usize)) };
    table
        .iter()
        .filter(|entry| entry.f_flags & libc::MNT_LOCAL as u32 != 0)
        .map(|entry| Mount {
            device: text(&entry.f_mntfromname),
            mount: text(&entry.f_mntonname),
            filesystem: text(&entry.f_fstypename),
            total_bytes: entry.f_blocks.saturating_mul(u64::from(entry.f_bsize)),
            available_bytes: entry.f_bavail.saturating_mul(u64::from(entry.f_bsize)),
        })
        .filter(|mount| !PSEUDO.contains(&mount.filesystem.as_str()))
        .collect()
}

/// The whole disk a volume lives on: `/dev/disk3s1s1` is on `disk3`. Every
/// volume of one APFS container shares that disk's IO counters.
pub(super) fn whole_disk(device: &str) -> Option<&str> {
    let name = device.strip_prefix("/dev/")?;
    let digits = name.strip_prefix("disk")?;
    let end = digits
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(digits.len());
    (end > 0).then(|| &name[..4 + end])
}

/// The device a mount point is mounted from.
pub(super) fn device_of(mount: &std::path::Path) -> Option<String> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(mount.as_os_str().as_bytes()).ok()?;
    let mut entry = std::mem::MaybeUninit::<libc::statfs>::zeroed();
    // SAFETY: `path` is NUL-terminated and `entry` is a writable statfs.
    if unsafe { libc::statfs(path.as_ptr(), entry.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: statfs returned 0, so it filled `entry`.
    Some(text(&unsafe { entry.assume_init() }.f_mntfromname))
}

fn text(raw: &[libc::c_char]) -> String {
    let bytes: Vec<u8> = raw
        .iter()
        .take_while(|byte| **byte != 0)
        .map(|byte| *byte as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}
