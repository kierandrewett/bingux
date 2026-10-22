.pragma library

// Client-side mirror of the plan schema v1 rules in distro/installer/README.md.
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
