{
    enable = true;

    # Declaring remotes replaces nix-flatpak's implicit Flathub default.
    remotes = [
        {
            name = "flathub";
            location = "https://dl.flathub.org/repo/";
        }
        {
            name = "flathub-beta";
            location = "https://dl.flathub.org/beta-repo/";
        }
        {
            name = "feather";
            location = "https://featherwallet.org/flatpak";
        }
        {
            name = "firefox-nightly";
            location = "https://kierandrewett.github.io/firefox-nightly-flatpak/";
        }
        {
            name = "marcterm";
            location = "https://marc2332.github.io/term";
        }
        {
            name = "orion-beta";
            location = "https://flatpak.orionbrowser.com/repo/beta/";
        }
        {
            name = "silverbullet";
            location = "https://releases.silverbullet.plus/flatpak/repo";
        }
    ];

    apps = [
        # Applications previously installed from Flathub.
        { appId = "app.drey.Warp"; }
        { appId = "app.zen_browser.zen"; }
        { appId = "com.bambulab.BambuStudio"; }
        { appId = "com.collaboraoffice.Office"; }
        { appId = "com.dec05eba.gpu_screen_recorder"; }
        { appId = "com.discordapp.Discord"; }
        { appId = "com.fastmail.Fastmail"; }
        { appId = "com.geeks3d.furmark"; }
        { appId = "com.github.k4zmu2a.spacecadetpinball"; }
        { appId = "com.github.tchx84.Flatseal"; }
        { appId = "com.github.wwmm.easyeffects"; }
        { appId = "com.google.Chrome"; }
        { appId = "com.google.EarthPro"; }
        { appId = "com.jetbrains.DataGrip"; }
        { appId = "com.konstantintutsch.Lock"; }
        { appId = "com.mattjakeman.ExtensionManager"; }
        { appId = "com.microsoft.Edge"; }
        { appId = "com.obsproject.Studio"; }
        { appId = "com.rustdesk.RustDesk"; }
        { appId = "com.spotify.Client"; }
        { appId = "com.usebottles.bottles"; }
        { appId = "com.vivaldi.Vivaldi"; }
        { appId = "dev.zed.Zed-Preview"; }
        { appId = "io.github.Archeb.opentrace"; }
        { appId = "io.github.kolunmi.Bazaar"; }
        { appId = "io.github.qwersyk.Newelle"; }
        { appId = "io.github.realmazharhussain.GdmSettings"; }
        { appId = "io.github.spacingbat3.webcord"; }
        { appId = "io.github.swordpuffin.rewaita"; }
        { appId = "io.github.wartybix.Constrict"; }
        { appId = "io.missioncenter.MissionCenter"; }
        { appId = "it.mijorus.gearlever"; }
        { appId = "md.obsidian.Obsidian"; }
        { appId = "net.sonobus.SonoBus"; }
        { appId = "no.mifi.losslesscut"; }
        { appId = "org.gnome.Boxes"; }
        { appId = "org.gnome.design.IconLibrary"; }
        { appId = "org.gnome.gitlab.somas.Apostrophe"; }
        { appId = "org.localsend.localsend_app"; }
        { appId = "org.nickvision.tubeconverter"; }
        { appId = "org.pgadmin.pgadmin4"; }
        { appId = "org.prismlauncher.PrismLauncher"; }
        { appId = "org.remmina.Remmina"; }
        { appId = "org.signal.Signal"; }
        { appId = "org.telegram.desktop"; }
        { appId = "org.xonotic.Xonotic"; }
        { appId = "rest.insomnia.Insomnia"; }

        # Applications from profile-specific Flatpak remotes.
        {
            appId = "com.kagi.Orion";
            origin = "orion-beta";
        }
        {
            appId = "io.marc.term";
            origin = "marcterm";
        }
        {
            appId = "org.featherwallet.Feather";
            origin = "feather";
        }
        {
            appId = "org.mozilla.FirefoxNightly";
            origin = "firefox-nightly";
        }
        {
            appId = "plus.silverbullet.desktop";
            origin = "silverbullet";
        }
    ];
}
