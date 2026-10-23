.pragma library

// Client-side mirror of the plan schema v2 rules in distro/installer/README.md.
// The daemon has no validate op, so the UI checks every field before it offers
// Continue; the daemon remains the authority and re-checks on install.

var MIN_PASSWORD = 8;
var MIN_PASSPHRASE = 8;
var MIN_DISK_BYTES = 8 * 1024 * 1024 * 1024;

function validHostname(value) {
    if (value.length < 1 || value.length > 63)
        return "Use 1 to 63 characters.";
    if (!/^[a-z0-9]([a-z0-9-]*[a-z0-9])?$/i.test(value))
        return "Use letters, digits and hyphens; start and end with a letter or digit.";
    return "";
}

function validUsername(value) {
    if (!/^[a-z_][a-z0-9_-]{0,31}$/.test(value))
        return "Start with a lowercase letter; use lowercase letters, digits, _ and -; up to 32 characters.";
    if (value === "root")
        return "root is reserved.";
    return "";
}

function validFullName(value) {
    if (value.length > 128)
        return "At most 128 characters.";
    if (/[\u0000-\u001f\u007f:,]/.test(value))
        return "Control characters, ':' and ',' are not allowed.";
    return "";
}

function validPassword(value) {
    if (value.length < MIN_PASSWORD)
        return "At least " + MIN_PASSWORD + " characters.";
    if (/[\u0000-\u001f\u007f]/.test(value))
        return "Control characters are not allowed.";
    return "";
}

function validPassphrase(value) {
    if (value.length < MIN_PASSPHRASE)
        return "At least " + MIN_PASSPHRASE + " characters.";
    return "";
}

function validLocale(value) {
    return /^[A-Za-z]{2,3}_[A-Za-z]{2}(\.UTF-8)?$/.test(value);
}

function validKeymap(value) {
    return /^[A-Za-z0-9_-]{1,64}$/.test(value);
}

function validTimezone(value) {
    return /^[A-Za-z_]+\/[A-Za-z0-9_+\-/]+$/.test(value);
}

// Strength in 0..4 from length and character classes. Feedback is plain text.
function passwordStrength(value) {
    if (value.length === 0)
        return {score: 0, label: "Enter a password", ok: false};
    var classes = 0;
    if (/[a-z]/.test(value)) classes++;
    if (/[A-Z]/.test(value)) classes++;
    if (/[0-9]/.test(value)) classes++;
    if (/[^A-Za-z0-9]/.test(value)) classes++;
    var score = 0;
    if (value.length >= MIN_PASSWORD) score = 1;
    if (value.length >= MIN_PASSWORD && classes >= 2) score = 2;
    if (value.length >= 12 && classes >= 3) score = 3;
    if (value.length >= 16 && classes >= 3) score = 4;
    var labels = ["Too short", "Weak", "Fair", "Good", "Strong"];
    return {score: score, label: labels[score], ok: value.length >= MIN_PASSWORD};
}

// A starting username from the full name: lowercase ASCII letters and digits
// of the first word, prefixed with a letter when it starts with a digit.
function deriveUsername(fullName) {
    var first = fullName.trim().split(/\s+/)[0] || "";
    var name = first.toLowerCase().replace(/[^a-z0-9_-]/g, "");
    if (name.length === 0)
        return "";
    if (/^[0-9-]/.test(name))
        name = "u" + name;
    return name.slice(0, 32);
}

function formatBytes(bytes) {
    if (bytes === null || bytes === undefined)
        return "unknown size";
    var gib = bytes / (1024 * 1024 * 1024);
    if (gib >= 1)
        return gib.toFixed(gib >= 100 ? 0 : 1) + " GiB";
    return Math.max(1, Math.round(bytes / (1024 * 1024))) + " MiB";
}

// Human phase names for the progress page. Unknown phases show their raw name.
function phaseLabel(phase) {
    var labels = {
        validate: "Checking the plan",
        preflight: "Checking the system",
        partition: "Partitioning the disk",
        mount: "Mounting the target",
        copy: "Copying the system",
        strip: "Cleaning the copy",
        configure: "Configuring the system",
        bootloader: "Installing the boot loader",
        finish: "Finishing up"
    };
    return labels[phase] || phase;
}

// Error codes the daemon can send, mapped to the message the user sees before
// the daemon's own text (README failure list).
function errorTitle(code) {
    var titles = {
        cancelled: "Installation cancelled",
        busy: "Another installation is running",
        invalid_plan: "The settings were refused",
        disk_not_found: "The disk was not found",
        disk_is_live_medium: "That disk holds the installer",
        disk_too_small: "The disk is too small",
        other_os_present: "The disk holds another system",
        missing_tool: "A required tool is missing",
        unsupported: "This option is not supported",
        payload_failed: "Copying the system failed",
        storage_failed: "Preparing the disk failed",
        config_failed: "Configuring the system failed",
        bootloader_failed: "Installing the boot loader failed",
        unavailable: "The installer is not reachable",
        internal: "Something went wrong"
    };
    return titles[code] || "Installation failed";
}

function nowStamp() {
    var d = new Date();
    function two(n) { return (n < 10 ? "0" : "") + n; }
    return d.getFullYear() + two(d.getMonth() + 1) + two(d.getDate()) + "-" + two(d.getHours()) + two(d.getMinutes()) + two(d.getSeconds());
}

// Manual storage (plan schema v2, storage.partitions). These mirror the daemon's
// rules in distro/installer/bingux_installer/plan.py; the daemon checks again.
var PARTITION_ROLES = ["esp", "bios_boot", "boot", "root", "home", "swap"];
var ROLE_ACTIONS = {
    esp: ["reuse", "format", "create"],
    bios_boot: ["reuse", "create"],
    boot: ["format", "create"],
    root: ["format", "create"],
    home: ["reuse", "format", "create"],
    swap: ["format", "create"]
};
var HOME_FILESYSTEMS = ["ext4", "btrfs", "xfs"];
var CREATE_MIN = 64 * 1024 * 1024;
var ROOT_CREATE_MIN = 1024 * 1024 * 1024;

var ROLE_NAMES = {
    esp: "EFI system partition",
    bios_boot: "BIOS boot partition",
    boot: "/boot partition",
    root: "root partition",
    home: "home partition",
    swap: "swap partition"
};

// The partition path for a number on a disk: /dev/sda3, /dev/nvme0n1p3.
function partitionPath(disk, number) {
    return /\d$/.test(disk) ? disk + "p" + number : disk + number;
}

// The partition number at the end of a path (/dev/sda3 -> 3); 0 when none.
function partitionNumber(path) {
    var m = /(\d+)$/.exec(path || "");
    return m ? parseInt(m[1], 10) : 0;
}

// The lowest partition number that is neither on the disk nor used by an entry.
function freePartitionNumber(disk, entries) {
    var taken = {};
    (disk.partitions || []).forEach(function (p) { taken[partitionNumber(p.path)] = true; });
    entries.forEach(function (e) { if (e.device) taken[partitionNumber(e.device)] = true; });
    var n = 1;
    while (taken[n])
        n++;
    return n;
}

function existingPartition(disk, path) {
    var list = disk && disk.partitions ? disk.partitions : [];
    for (var i = 0; i < list.length; i++)
        if (list[i].path === path)
            return list[i];
    return null;
}

// Every problem with a manual partition list, as sentences. [] when the list
// is valid. ctx: {disk (probe object), uefi (bool), encrypted (bool)}.
function manualProblems(entries, ctx) {
    var out = [];
    var disk = ctx.disk;
    var devices = {};
    var counts = {};
    var create = 0;
    if (!disk)
        return ["Choose a disk first."];
    entries.forEach(function (e) {
        var name = ROLE_NAMES[e.role] || e.role;
        counts[e.role] = (counts[e.role] || 0) + 1;
        if (!e.device) {
            out.push("Choose a partition for the " + name + ".");
            return;
        }
        if (devices[e.device])
            out.push("Each device may appear only once (" + e.device + ").");
        devices[e.device] = true;
        if (!ROLE_ACTIONS[e.role] || ROLE_ACTIONS[e.role].indexOf(e.action) < 0)
            out.push(e.action + " is not allowed for the " + name + ".");
        if (e.action === "create") {
            create += e.sizeBytes;
            if (e.sizeBytes < CREATE_MIN)
                out.push("The " + name + " is smaller than 64 MiB.");
            if (e.role === "root" && e.sizeBytes < ROOT_CREATE_MIN)
                out.push("The root partition needs at least 1 GiB.");
            if (existingPartition(disk, e.device))
                out.push(e.device + " already exists; choose reuse or format for it.");
            return;
        }
        var part = existingPartition(disk, e.device);
        if (!part) {
            out.push(e.device + " is not a partition of " + disk.path + ".");
            return;
        }
        if (e.action === "format" && part.mountpoint)
            out.push(e.device + " is mounted at " + part.mountpoint + "; unmount it or pick another partition.");
        if (e.action === "reuse" && e.role === "esp" && part.fstype !== "vfat")
            out.push(e.device + " must be FAT (vfat) to reuse it as the EFI partition.");
        if (e.action === "reuse" && e.role === "home" && HOME_FILESYSTEMS.indexOf(part.fstype) < 0)
            out.push(e.device + " must be ext4, btrfs or xfs to reuse it as home.");
    });
    PARTITION_ROLES.forEach(function (role) {
        if ((counts[role] || 0) > 1)
            out.push("At most one " + (ROLE_NAMES[role] || role) + " is allowed.");
    });
    if (counts.root !== 1)
        out.push("Exactly one root partition is required.");
    if (ctx.uefi) {
        if (counts.esp !== 1)
            out.push("UEFI needs exactly one EFI system partition.");
        if (counts.bios_boot || counts.boot)
            out.push("A BIOS boot or /boot partition is only used with BIOS.");
    } else {
        if (counts.bios_boot !== 1)
            out.push("BIOS needs exactly one BIOS boot partition.");
        if (counts.esp)
            out.push("An EFI system partition is only used with UEFI.");
        if (ctx.encrypted && !counts.boot)
            out.push("BIOS with encryption needs a /boot partition.");
    }
    if (ctx.encrypted && counts.home)
        out.push("A separate home partition cannot be encrypted yet.");
    var free = disk.free_bytes;
    if (create > 0 && (free === null || free === undefined))
        out.push("The unallocated space is unknown, so new partitions cannot be checked.");
    else if (create > 0 && create > free)
        out.push("The new partitions need " + formatBytes(create) + "; the disk has " + formatBytes(free) + " unallocated.");
    return out;
}
