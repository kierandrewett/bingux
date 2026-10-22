pragma Singleton
import QtQuick

// Every value the installer collects, plus the probe and install status the
// pages show. Pages write here, so Back keeps what the user chose. planObject()
// builds the plan schema v2 document (distro/installer/README.md). The probe's
// capabilities decide which choices the pages offer.
QtObject {
    id: root

    readonly property var localeChoices: [
        {locale: "en_GB.UTF-8", name: "English (United Kingdom)", keymap: "uk", timezone: "Europe/London"},
        {locale: "en_US.UTF-8", name: "English (United States)", keymap: "us", timezone: "America/New_York"},
        {locale: "de_DE.UTF-8", name: "Deutsch (Deutschland)", keymap: "de", timezone: "Europe/Berlin"},
        {locale: "fr_FR.UTF-8", name: "Francais (France)", keymap: "fr", timezone: "Europe/Paris"},
        {locale: "es_ES.UTF-8", name: "Espanol (Espana)", keymap: "es", timezone: "Europe/Madrid"},
        {locale: "it_IT.UTF-8", name: "Italiano (Italia)", keymap: "it", timezone: "Europe/Rome"},
        {locale: "nl_NL.UTF-8", name: "Nederlands (Nederland)", keymap: "nl", timezone: "Europe/Amsterdam"},
        {locale: "pt_BR.UTF-8", name: "Portugues (Brasil)", keymap: "br-abnt2", timezone: "America/Sao_Paulo"}
    ]
    readonly property var keymapChoices: [
        {keymap: "uk", name: "British"},
        {keymap: "us", name: "U.S."},
        {keymap: "de", name: "German"},
        {keymap: "fr", name: "French"},
        {keymap: "es", name: "Spanish"},
        {keymap: "it", name: "Italian"},
        {keymap: "nl", name: "Dutch"},
        {keymap: "br-abnt2", name: "Portuguese (Brazil, ABNT2)"}
    ]
    // Bundled avatar files in assets/, shown on the account page.
    readonly property var avatarChoices: ["avatar-1", "avatar-2", "avatar-3", "avatar-4", "avatar-5", "avatar-6"]
    readonly property int gib: 1024 * 1024 * 1024
    // What a probe without a capabilities field means (README, Protocol): the Shape
    // listed there, which is what this build implements.
    readonly property var baseCapabilities: ({
        storage_modes: ["erase"],
        filesystems: ["ext4", "btrfs", "xfs"],
        swap: ["none", "zram", "file", "partition"],
        encryption: {uefi: true, bios: true},
        dual_boot: false
    })

    // Language and input. locale and keymap go into the plan.
    property string locale: "en_GB.UTF-8"
    property string keymap: "uk"
    // Time zone and clock. timezone and ntp go into the plan.
    property string timezone: "Europe/London"
    property bool ntp: true

    // Identity. hostname, the first user and their password go into the plan.
    property string hostname: "bingux"
    property string fullName: ""
    property string username: ""
    property bool usernameEdited: false
    property string password: ""
    property string avatar: "avatar-1"
    // autoLogin starts the user's session at boot (plan autologin). rootPasswordSet
    // turns on rootPassword (plan root_password); otherwise the root account is locked.
    property bool autoLogin: false
    property bool rootPasswordSet: false
    property string rootPassword: ""

    // Disk. storageMode is erase, free_space or alongside; shrinkPartition and
    // shrinkBytes (the new size of that partition) are used by alongside only.
    property string diskPath: ""
    property string storageMode: "erase"
    property string shrinkPartition: ""
    property real shrinkBytes: 0
    property string filesystem: "ext4"
    property string swap: "none"
    property int swapGiB: 4
    property bool encryption: false
    property string passphrase: ""
    property bool otherOsAllow: false

    // Probe results from the daemon.
    property string probeState: "idle"  // idle | loading | ready | failed
    property string probeError: ""
    property string firmware: ""
    property var disks: []
    property var capabilities: baseCapabilities
    property string liveDisk: ""
    property var toolsMissing: []
    property string installerVersion: ""

    // Install status, written by InstallerClient.
    property string installState: "idle"  // idle | running | done | failed
    property string phase: ""
    property real percent: -1
    property string phaseMessage: ""
    property var logLines: []
    property var result: null
    property string errorCode: ""
    property string errorMessage: ""

    readonly property var selectedDisk: {
        for (const disk of disks)
            if (disk.path === diskPath)
                return disk;
        return null;
    }
    // True when the probe lists the choice; the pages offer only those.
    function offers(kind, value) {
        const list = capabilities[kind] || [];
        return list.indexOf(value) >= 0;
    }
    function encryptionOffered() {
        return firmware !== "" && capabilities.encryption !== undefined && capabilities.encryption[firmware] === true;
    }

    // Takes the probe's capabilities, filling in what it leaves out, and drops
    // any choice made earlier that this build no longer offers.
    function setCapabilities(raw) {
        const merged = {};
        for (const key of Object.keys(baseCapabilities))
            merged[key] = raw && raw[key] !== undefined ? raw[key] : baseCapabilities[key];
        capabilities = merged;
        if (!offers("storage_modes", storageMode))
            storageMode = "erase";
        if (!offers("filesystems", filesystem))
            filesystem = "ext4";
        if (!offers("swap", swap))
            swap = "none";
        if (!encryptionOffered())
            encryption = false;
    }

    readonly property var localeChoice: {
        for (const choice of localeChoices)
            if (choice.locale === locale)
                return choice;
        return null;
    }

    // Applies a language choice, moving the keymap and time zone with it.
    function chooseLocale(choice) {
        locale = choice.locale;
        keymap = choice.keymap;
        timezone = choice.timezone;
    }

    function appendLog(text) {
        const lines = logLines.concat([text]);
        logLines = lines.length > 2000 ? lines.slice(lines.length - 2000) : lines;
    }

    function resetInstall() {
        installState = "idle";
        phase = "";
        percent = -1;
        phaseMessage = "";
        logLines = [];
        result = null;
        errorCode = "";
        errorMessage = "";
    }

    // The storage object of the plan. The validator refuses the fields of other modes.
    function storageObject() {
        if (storageMode === "alongside")
            return {mode: "alongside", shrink: {partition: shrinkPartition, size_bytes: Math.round(shrinkBytes)}};
        return {mode: storageMode};
    }

    // The plan schema v2 document (README, Plan schema v2). Fields the chosen
    // storage mode does not allow are left out.
    function planObject() {
        const erase = storageMode === "erase";
        const plan = {
            version: 2,
            disk: diskPath,
            firmware: "auto",
            storage: storageObject(),
            filesystem: filesystem,
            encryption: encryption ? {enabled: true, passphrase: passphrase} : {enabled: false},
            swap: swap,
            hostname: hostname,
            timezone: timezone,
            locale: locale,
            keymap: keymap,
            ntp: ntp,
            root_password: rootPasswordSet ? rootPassword : null,
            autologin: autoLogin ? username : null,
            users: [{name: username, password: password, admin: true, full_name: fullName}]
        };
        if (erase) {
            plan.confirm_erase = diskPath;
            plan.other_os = otherOsAllow ? "allow" : "refuse";
        }
        if (swap === "file" || swap === "partition")
            plan.swap_bytes = swapGiB * gib;
        return plan;
    }
}
