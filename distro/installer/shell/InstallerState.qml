pragma Singleton
import QtQuick

// Every value the installer collects, plus the probe and install status the
// pages show. Pages write here, so Back keeps what the user chose. planObject()
// builds the plan schema v1 document; fields the protocol cannot carry yet are
// kept here for the pages but never sent (see the disabled rows in the pages).
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

    // Language and input. locale and keymap go into the plan.
    property string locale: "en_GB.UTF-8"
    property string keymap: "uk"
    // Time zone goes into the plan. ntp is UI-only until the protocol carries it.
    property string timezone: "Europe/London"
    property bool ntp: true

    // Identity. hostname, the first user and their password go into the plan.
    property string hostname: "bingux"
    property string fullName: ""
    property string username: ""
    property bool usernameEdited: false
    property string password: ""
    property string avatar: "avatar-1"
    // UI-only until the protocol carries them.
    property bool autoLogin: false
    property bool rootPassword: false

    // Disk. Only a whole-disk erase is in the protocol.
    property string diskPath: ""
    property string filesystem: "ext4"
    property bool encryption: false
    property string passphrase: ""
    property bool otherOsAllow: false

    // Probe results from the daemon.
    property string probeState: "idle"  // idle | loading | ready | failed
    property string probeError: ""
    property string firmware: ""
    property var disks: []
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

    // The plan schema v1 document. Only protocol fields appear here.
    function planObject() {
        const plan = {
            version: 1,
            disk: diskPath,
            confirm_erase: diskPath,
            firmware: "auto",
            filesystem: filesystem,
            encryption: encryption ? {enabled: true, passphrase: passphrase} : {enabled: false},
            other_os: otherOsAllow ? "allow" : "refuse",
            hostname: hostname,
            timezone: timezone,
            locale: locale,
            keymap: keymap,
            users: [{name: username, password: password, admin: true, full_name: fullName}]
        };
        return plan;
    }
}
