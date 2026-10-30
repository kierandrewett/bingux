pragma Singleton
import QtQuick
import "InstallerSchema.js" as Schema

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

    // Disk. storageMode is erase, free_space, alongside or manual; shrinkPartition
    // and shrinkBytes (the new size of that partition) are used by alongside only.
    // manualEntries is the partition list of manual mode: {device, action, role,
    // sizeBytes, fs}. It is replaced, never edited in place, so bindings update.
    property string diskPath: ""
    property string storageMode: "erase"
    property string shrinkPartition: ""
    property real shrinkBytes: 0
    property var manualEntries: []
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
    // Scale of the wide card (1 at the compact size); the text helpers read it.
    property real uiScale: 1

    // Install status, written by InstallerClient.
    property string installState: "idle"  // idle | running | done | failed
    property string phase: ""
    property real percent: -1
    property string phaseMessage: ""
    // The install log, read by InstallerLog and the error page. A ListModel, so
    // appendLog adds a line without copying the others. At most logLimit lines are
    // held; when a line would pass it, the oldest lines go in one batch down to
    // logKeep, so the removal cost is paid once per (logLimit - logKeep) appends.
    readonly property int logLimit: 2000
    readonly property int logKeep: 1500
    readonly property ListModel logModel: ListModel {}
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
    // The probe's firmware decides UEFI or BIOS rules; "" until the probe answers.
    readonly property bool uefi: firmware === "uefi"
    function manualHasRole(role) {
        return manualEntries.some(e => e.role === role);
    }
    // Swap follows the manual list in manual mode: a swap role entry means
    // "partition"; without one, the partition choice falls back to none.
    function effectiveSwap() {
        if (storageMode !== "manual")
            return swap;
        if (manualHasRole("swap"))
            return "partition";
        return swap === "partition" ? "none" : swap;
    }
    // Bumped when an entry changes in place (a size typed in, for one), so the
    // bindings that check the list run again. Replacing manualEntries does too.
    property int manualRevision: 0
    function touchManual() {
        manualRevision += 1;
    }
    // A size typed in GiB (sizeText, so a half-typed "0." is kept) as bytes; 0 when not a number above 0.
    function entrySizeBytes(entry) {
        const v = parseFloat(entry.sizeText);
        return v > 0 ? Math.round(v * gib) : 0;
    }
    // The list with sizes in bytes, for the schema mirror.
    function checkedEntries() {
        return manualEntries.map(e => Object.assign({}, e, {sizeBytes: e.action === "create" ? entrySizeBytes(e) : 0}));
    }
    // The manual list as plan partitions (README, Plan schema v2).
    function manualPlanPartitions() {
        return manualEntries.map(e => {
            const part = {device: e.device, action: e.action, role: e.role};
            if (e.action === "create")
                part.size_bytes = entrySizeBytes(e);
            if (e.role === "home" && e.action !== "reuse")
                part.fs = e.fs;
            return part;
        });
    }
    // Problems with the manual list, from the schema mirror; [] when it can be installed.
    function manualProblems() {
        manualRevision;
        return Schema.manualProblems(checkedEntries(), {disk: selectedDisk, uefi: uefi, encrypted: encryption});
    }
    // The swap choice from the storage page. Leaving partition swap in manual mode
    // drops the swap role entry, so the list and the choice agree.
    function setSwap(value) {
        swap = value;
        if (storageMode === "manual" && value !== "partition")
            manualEntries = manualEntries.filter(e => e.role !== "swap");
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
        logModel.append({line: String(text)});
        if (logModel.count > logLimit)
            logModel.remove(0, logModel.count - logKeep);
    }

    // The whole log as one string, for saving it. Only the error page calls this.
    function logText() {
        const lines = [];
        for (let i = 0; i < logModel.count; i++)
            lines.push(logModel.get(i).line);
        return lines.join("\n");
    }

    function resetInstall() {
        installState = "idle";
        phase = "";
        percent = -1;
        phaseMessage = "";
        logModel.clear();
        result = null;
        errorCode = "";
        errorMessage = "";
    }

    // The storage object of the plan. The validator refuses the fields of other modes.
    function storageObject() {
        if (storageMode === "alongside")
            return {mode: "alongside", shrink: {partition: shrinkPartition, size_bytes: Math.round(shrinkBytes)}};
        if (storageMode === "manual")
            return {mode: "manual", partitions: manualPlanPartitions()};
        return {mode: storageMode};
    }

    // The plan schema v2 document (README, Plan schema v2). Fields the chosen
    // storage mode does not allow are left out.
    function planObject() {
        const erase = storageMode === "erase";
        const confirmed = erase || storageMode === "manual";
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
        if (confirmed)
            plan.confirm_erase = diskPath;
        if (erase)
            plan.other_os = otherOsAllow ? "allow" : "refuse";
        // A manual swap partition is the swap role entry; its size is that entry's.
        plan.swap = effectiveSwap();
        if (plan.swap === "file" || (plan.swap === "partition" && storageMode !== "manual"))
            plan.swap_bytes = swapGiB * gib;
        return plan;
    }
}
