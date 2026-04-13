{ lib, modulesPath, pkgs, ... }:
let
    bingux-plymouth = pkgs.callPackage ../../pkgs/bingux-plymouth { };

    bingux-install = pkgs.writeShellScriptBin "bingux-install" ''
        set -uo pipefail
        TOTAL_STEPS=7
        LOG_FILE="/tmp/bingux-install-$(date +%Y%m%d-%H%M%S).log"

        # Log all output to a file while preserving terminal colors
        if [[ -z "''${BINGUX_LOGGING:-}" ]]; then
            export BINGUX_LOGGING=1
            exec ${pkgs.util-linux}/bin/script -qfc "$(readlink -f "$0")" "$LOG_FILE"
        fi

        # ── Colors ──
        BOLD=$'\e[1m'
        DIM=$'\e[2m'
        RED=$'\e[31m'
        GREEN=$'\e[32m'
        YELLOW=$'\e[33m'
        BLUE=$'\e[34m'
        CYAN=$'\e[36m'
        WHITE=$'\e[37m'
        RESET=$'\e[0m'

        # ── UI helpers ──
        info()    { echo "  ''${BLUE}::''${RESET} $*"; }
        success() { echo "  ''${GREEN}✓''${RESET} $*"; }
        warn()    { echo "  ''${YELLOW}!''${RESET} $*"; }
        fail()    { echo "  ''${RED}✗''${RESET} $*"; }
        prompt()  { read -p "  ''${BOLD}▸''${RESET} $1" "$2"; }

        header() {
            local current="$1"
            local title="$2"
            clear
            echo ""
            echo "  ''${BOLD}''${BLUE}Bingux Installer''${RESET}"
            echo "  ''${DIM}$(printf '─%.0s' $(seq 1 50))''${RESET}"
            echo ""

            # Progress bar
            local filled=$(( current * 50 / TOTAL_STEPS ))
            local empty=$(( 50 - filled ))
            printf "  ''${BLUE}"
            printf '█%.0s' $(seq 1 $filled)
            printf "''${DIM}"
            [[ $empty -gt 0 ]] && printf '░%.0s' $(seq 1 $empty)
            printf "''${RESET}  ''${DIM}%d/%d''${RESET}\n" "$current" "$TOTAL_STEPS"
            echo ""
            echo "  ''${BOLD}''${WHITE}$title''${RESET}"
            echo ""
        }

        wait_enter() {
            echo ""
            prompt "Press Enter to continue... " _
        }

        prompt_with_default() {
            local label="$1"
            local default="$2"
            local varname="$3"
            if [[ -n "$default" ]]; then
                prompt "$label [''${BOLD}$default''${RESET}]: " _input
                eval "$varname=\"''${_input:-$default}\""
            else
                prompt "$label: " _input
                eval "$varname=\"$_input\""
            fi
        }

        validate_block_device() {
            local dev="$1"
            if [[ -z "$dev" ]] || [[ ! -b "$dev" ]]; then
                fail "''${BOLD}$dev''${RESET} is not a valid block device."
                exit 1
            fi
        }

        detect_partitions() {
            local disk
            disk=$(lsblk -dnpo NAME,TYPE,SIZE \
                | awk '$2 == "disk" { print $3, $1 }' \
                | sort -rh | head -1 | awk '{print $2}')
            [[ -z "$disk" ]] && return

            DETECTED_DISK="$disk"
            DETECTED_EFI=""
            DETECTED_ROOT=""
            DETECTED_SWAP=""

            while IFS= read -r part; do
                local fstype size_b parttype
                fstype=$(lsblk -npo FSTYPE "$part" 2>/dev/null | head -1 | xargs)
                size_b=$(lsblk -bnpo SIZE "$part" 2>/dev/null | head -1 | xargs)
                parttype=$(lsblk -npo PARTTYPE "$part" 2>/dev/null | head -1 | xargs)

                if [[ -z "$DETECTED_EFI" ]]; then
                    if [[ "$parttype" == "c12a7328-f81f-11d2-ba4b-00a0c93ec93b" ]]; then
                        DETECTED_EFI="$part"; continue
                    elif [[ "$fstype" == "vfat" ]] && [[ -n "$size_b" ]] && (( size_b <= 2147483648 )); then
                        DETECTED_EFI="$part"; continue
                    fi
                fi

                if [[ "$fstype" == "swap" ]] || [[ "$parttype" == "0657fd6d-a4ab-43c4-84e5-0933c84b4f4f" ]]; then
                    DETECTED_SWAP="$part"; continue
                fi

                if [[ -z "$DETECTED_ROOT" ]] && [[ "$fstype" != "vfat" ]]; then
                    DETECTED_ROOT="$part"
                fi
            done < <(lsblk -nplo NAME "$disk" | tail -n +2)
        }

        cleanup() {
            local rc=$?
            if [[ $rc -ne 0 ]]; then
                echo ""
                fail "The installer exited unexpectedly (code $rc)."
                echo ""
                echo "  Log saved to: ''${BOLD}$LOG_FILE''${RESET}"
                echo "  To retry, run: ''${BOLD}sudo bingux-install''${RESET}"
                echo ""
                read -p "  Press Enter to view log..." _
                ${pkgs.gnome-text-editor}/bin/gnome-text-editor "$LOG_FILE" &
            else
                info "Log saved to: ''${DIM}$LOG_FILE''${RESET}"
            fi
        }
        trap cleanup EXIT

        if [[ $EUID -ne 0 ]]; then
            clear
            echo ""
            fail "Must run as root: ''${BOLD}sudo bingux-install''${RESET}"
            exit 1
        fi

        # ══════════════════════════════════════════
        # Step 1: GitHub Authentication
        # ══════════════════════════════════════════
        header 1 "GitHub Authentication"

        if ! ${pkgs.gh}/bin/gh auth status &>/dev/null; then
            info "Sign in to GitHub to access your config repository."
            echo ""
            while ! ${pkgs.gh}/bin/gh auth login --git-protocol https; do
                warn "Authentication failed. Try again."
            done
            GH_TOKEN=$(${pkgs.gh}/bin/gh auth token 2>/dev/null)
            if [[ -n "$GH_TOKEN" ]]; then
                mkdir -p /root/.config/nix
                echo "access-tokens = github.com=$GH_TOKEN" >> /root/.config/nix/nix.conf
            fi
            success "GitHub authenticated."
        else
            success "GitHub already authenticated."
        fi

        # ══════════════════════════════════════════
        # Step 2: Repository & Host Selection
        # ══════════════════════════════════════════
        header 2 "Configuration Repository"

        echo "  Enter the URL of your NixOS config repository."
        echo "  ''${DIM}e.g. https://github.com/user/nixos-config''${RESET}"
        echo ""
        prompt_with_default "Repository URL" "" REPO_URL
        if [[ -z "$REPO_URL" ]]; then
            fail "No repository URL provided."
            exit 1
        fi

        info "Cloning ''${BOLD}$REPO_URL''${RESET}..."
        rm -rf /tmp/bingux-os
        ${pkgs.git}/bin/git clone "$REPO_URL" /tmp/bingux-os
        success "Repository cloned."

        echo ""
        info "Enumerating available hosts..."
        mapfile -t HOSTS < <(${pkgs.nix}/bin/nix flake show --json /tmp/bingux-os 2>/dev/null \
            | ${pkgs.jq}/bin/jq -r '.nixosConfigurations // {} | keys[]' 2>/dev/null)

        if [[ ''${#HOSTS[@]} -eq 0 ]]; then
            fail "No NixOS configurations found in the repository."
            fail "Ensure the flake exports nixosConfigurations."
            exit 1
        fi

        echo ""
        echo "  ''${BOLD}Available hosts:''${RESET}"
        echo ""
        for i in "''${!HOSTS[@]}"; do
            printf "    ''${GREEN}%d''${RESET})  ''${BOLD}%s''${RESET}\n" "$((i+1))" "''${HOSTS[$i]}"
        done
        echo ""

        if [[ ''${#HOSTS[@]} -eq 1 ]]; then
            TARGET_HOST="''${HOSTS[0]}"
            info "Only one host found, selecting ''${BOLD}$TARGET_HOST''${RESET}."
        else
            prompt "Select host [1-''${#HOSTS[@]}]: " HOST_IDX
            HOST_IDX=$((HOST_IDX - 1))
            if [[ $HOST_IDX -lt 0 ]] || [[ $HOST_IDX -ge ''${#HOSTS[@]} ]]; then
                fail "Invalid selection."
                exit 1
            fi
            TARGET_HOST="''${HOSTS[$HOST_IDX]}"
        fi

        success "Target: ''${BOLD}$TARGET_HOST''${RESET}"

        # ══════════════════════════════════════════
        # Step 3: Installation Mode
        # ══════════════════════════════════════════
        header 3 "Installation Mode"

        echo "  ''${BOLD}Choose an installation mode:''${RESET}"
        echo ""
        echo "    ''${GREEN}1''${RESET})  ''${BOLD}Guided''${RESET}    Partitioning, formatting & encryption"
        echo "    ''${YELLOW}2''${RESET})  ''${BOLD}Manual''${RESET}    GParted + shell, you set up /mnt"
        echo "    ''${CYAN}3''${RESET})  ''${BOLD}Install''${RESET}   /mnt is already mounted, skip to install"
        echo ""
        prompt "Choice [1/2/3]: " MODE
        echo ""

        case "$MODE" in

        # ══════════════════════════════════════════
        # GUIDED
        # ══════════════════════════════════════════
        1)
            header 4 "Partitioning"

            echo "  GParted will open so you can create your partitions."
            echo ""
            echo "    ''${BOLD}Required:''${RESET}"
            echo "      ''${GREEN}•''${RESET} ~1 GB partition for ''${BOLD}EFI''${RESET}  (FAT32, boot flag)"
            echo "      ''${GREEN}•''${RESET} Partition for ''${BOLD}root''${RESET}       (rest of disk)"
            echo ""
            echo "    ''${DIM}Optional:''${RESET}"
            echo "      ''${DIM}•''${RESET} Separate /home partition"
            echo "      ''${DIM}•''${RESET} Swap partition"
            echo ""
            echo "  ''${DIM}Don't format anything — the wizard handles that next.''${RESET}"

            wait_enter
            ${pkgs.gparted}/bin/gparted || true

            detect_partitions

            header 5 "Select Partitions"

            echo "  ''${BOLD}Detected devices:''${RESET}"
            echo "  ''${DIM}$(printf '─%.0s' $(seq 1 50))''${RESET}"
            lsblk -o NAME,SIZE,FSTYPE,LABEL,MOUNTPOINT | grep -v loop | grep -v sr0 | sed 's/^/  /'
            echo "  ''${DIM}$(printf '─%.0s' $(seq 1 50))''${RESET}"
            echo ""

            if [[ -n "''${DETECTED_EFI:-}" ]] || [[ -n "''${DETECTED_ROOT:-}" ]]; then
                info "Partitions auto-detected — press Enter to accept defaults."
                echo ""
            fi

            prompt_with_default "EFI partition  (e.g. /dev/vda1)" "''${DETECTED_EFI:-}" EFI_DEV
            validate_block_device "$EFI_DEV"

            prompt_with_default "Root partition (e.g. /dev/vda2)" "''${DETECTED_ROOT:-}" ROOT_DEV
            validate_block_device "$ROOT_DEV"

            echo ""
            prompt "Separate /home partition? (blank = skip): " HOME_DEV
            if [[ -n "$HOME_DEV" ]]; then
                validate_block_device "$HOME_DEV"
            fi

            prompt_with_default "Swap partition? (blank = skip)" "''${DETECTED_SWAP:-}" SWAP_DEV
            if [[ -n "$SWAP_DEV" ]]; then
                validate_block_device "$SWAP_DEV"
            fi

            # Encryption & filesystem
            header 6 "Encryption & Filesystem"

            prompt "Encrypt root with LUKS? [y/N]: " use_luks
            ROOT_MOUNT="$ROOT_DEV"
            LUKS_ROOT=0
            if [[ "$use_luks" =~ ^[Yy]$ ]]; then
                LUKS_ROOT=1
            fi

            LUKS_HOME=0
            HOME_MOUNT="$HOME_DEV"
            if [[ -n "$HOME_DEV" ]]; then
                prompt "Encrypt /home with LUKS? [y/N]: " use_luks_home
                if [[ "$use_luks_home" =~ ^[Yy]$ ]]; then
                    LUKS_HOME=1
                fi
            fi

            echo ""
            prompt "Root filesystem (btrfs/ext4/xfs) [btrfs]: " ROOT_FS
            ROOT_FS="''${ROOT_FS:-btrfs}"

            # Confirm
            echo ""
            echo "  ''${BOLD}Please review your choices:''${RESET}"
            echo ""
            echo "    EFI partition:    ''${BOLD}$EFI_DEV''${RESET}  →  FAT32"
            if [[ "$LUKS_ROOT" == "1" ]]; then
                echo "    Root partition:   ''${BOLD}$ROOT_DEV''${RESET}  →  $ROOT_FS  ''${YELLOW}(LUKS encrypted)''${RESET}"
            else
                echo "    Root partition:   ''${BOLD}$ROOT_DEV''${RESET}  →  $ROOT_FS"
            fi
            if [[ -n "$HOME_DEV" ]]; then
                if [[ "$LUKS_HOME" == "1" ]]; then
                    echo "    Home partition:   ''${BOLD}$HOME_DEV''${RESET}  →  $ROOT_FS  ''${YELLOW}(LUKS encrypted)''${RESET}"
                else
                    echo "    Home partition:   ''${BOLD}$HOME_DEV''${RESET}  →  $ROOT_FS"
                fi
            fi
            if [[ -n "$SWAP_DEV" ]]; then
                echo "    Swap partition:   ''${BOLD}$SWAP_DEV''${RESET}"
            fi
            if [[ "$ROOT_FS" == "btrfs" ]]; then
                echo ""
                echo "    ''${DIM}Btrfs subvolumes: @, @nix, @home''${RESET}"
            fi
            echo ""
            echo "  ''${RED}''${BOLD}WARNING: This will erase data on the selected partitions!''${RESET}"
            echo ""
            prompt "Type ''${BOLD}yes''${RESET} to proceed: " confirm
            [[ "$confirm" == "yes" ]] || { warn "Aborted."; exit 1; }

            # Format & mount
            header 7 "Formatting & Mounting"

            if [[ "$LUKS_ROOT" == "1" ]]; then
                info "Setting up LUKS on ''${BOLD}$ROOT_DEV''${RESET}..."
                cryptsetup luksFormat --type luks2 "$ROOT_DEV"
                cryptsetup open "$ROOT_DEV" cryptroot
                ROOT_MOUNT="/dev/mapper/cryptroot"
                success "LUKS root ready."
            fi

            if [[ "$LUKS_HOME" == "1" ]] && [[ -n "$HOME_DEV" ]]; then
                info "Setting up LUKS on ''${BOLD}$HOME_DEV''${RESET}..."
                cryptsetup luksFormat --type luks2 "$HOME_DEV"
                cryptsetup open "$HOME_DEV" crypthome
                HOME_MOUNT="/dev/mapper/crypthome"
                success "LUKS home ready."
            fi

            info "Formatting EFI (''${BOLD}$EFI_DEV''${RESET})..."
            mkfs.fat -F 32 -n EFI "$EFI_DEV"
            efi_disk=$(lsblk -npo PKNAME "$EFI_DEV" | head -1)
            efi_partnum=$(cat "/sys/class/block/$(basename "$EFI_DEV")/partition" 2>/dev/null)
            if [[ -n "$efi_disk" ]] && [[ -n "$efi_partnum" ]]; then
                ${pkgs.gptfdisk}/bin/sgdisk -t "''${efi_partnum}:ef00" "$efi_disk" >/dev/null 2>&1 || true
            fi
            success "EFI formatted."

            info "Formatting root (''${BOLD}$ROOT_MOUNT''${RESET}) as ''${BOLD}$ROOT_FS''${RESET}..."
            case "$ROOT_FS" in
                btrfs) mkfs.btrfs -f -L nixos "$ROOT_MOUNT" ;;
                ext4)  mkfs.ext4 -F -L nixos "$ROOT_MOUNT" ;;
                xfs)   mkfs.xfs -f -L nixos "$ROOT_MOUNT" ;;
                *)     fail "Unsupported filesystem: $ROOT_FS"; exit 1 ;;
            esac
            success "Root formatted."

            if [[ -n "$HOME_DEV" ]]; then
                info "Formatting /home (''${BOLD}$HOME_MOUNT''${RESET}) as ''${BOLD}$ROOT_FS''${RESET}..."
                case "$ROOT_FS" in
                    btrfs) mkfs.btrfs -f -L home "$HOME_MOUNT" ;;
                    ext4)  mkfs.ext4 -F -L home "$HOME_MOUNT" ;;
                    xfs)   mkfs.xfs -f -L home "$HOME_MOUNT" ;;
                esac
                success "/home formatted."
            fi

            if [[ -n "$SWAP_DEV" ]]; then
                info "Setting up swap (''${BOLD}$SWAP_DEV''${RESET})..."
                mkswap -L swap "$SWAP_DEV"
                swapon "$SWAP_DEV"
                success "Swap enabled."
            fi

            echo ""
            if [[ "$ROOT_FS" == "btrfs" ]]; then
                info "Creating btrfs subvolumes..."
                mount "$ROOT_MOUNT" /mnt
                btrfs subvolume create /mnt/@
                btrfs subvolume create /mnt/@nix
                if [[ -z "$HOME_DEV" ]]; then
                    btrfs subvolume create /mnt/@home
                fi
                umount /mnt

                mount -o subvol=@,compress=zstd,noatime "$ROOT_MOUNT" /mnt
                mkdir -p /mnt/{boot,nix,home}
                mount -o subvol=@nix,compress=zstd,noatime "$ROOT_MOUNT" /mnt/nix
                if [[ -z "$HOME_DEV" ]]; then
                    mount -o subvol=@home,compress=zstd,noatime "$ROOT_MOUNT" /mnt/home
                fi
                success "Btrfs subvolumes created and mounted."
            else
                mount "$ROOT_MOUNT" /mnt
                mkdir -p /mnt/{boot,home}
                success "Root mounted."
            fi

            mount "$EFI_DEV" /mnt/boot
            success "EFI mounted at /mnt/boot."

            if [[ -n "$HOME_DEV" ]]; then
                mkdir -p /mnt/home
                mount "$HOME_MOUNT" /mnt/home
                success "/home mounted."
            fi
            ;;

        # ══════════════════════════════════════════
        # MANUAL
        # ══════════════════════════════════════════
        2)
            header 4 "Manual Setup"

            echo "  GParted will open for partitioning."
            echo "  After closing it, you'll get a shell."
            echo ""
            echo "  Partition, format, encrypt, and mount everything to ''${BOLD}/mnt''${RESET}."
            echo "  When done, type ''${BOLD}exit''${RESET} to continue."
            echo ""
            echo "  ''${DIM}Example:''${RESET}"
            echo "    cryptsetup luksFormat /dev/sda2"
            echo "    cryptsetup open /dev/sda2 cryptroot"
            echo "    mkfs.btrfs /dev/mapper/cryptroot"
            echo "    mount /dev/mapper/cryptroot /mnt"
            echo "    mkdir -p /mnt/boot && mount /dev/sda1 /mnt/boot"
            echo "    exit"

            wait_enter
            ${pkgs.gparted}/bin/gparted || true

            echo ""
            info "Dropping to shell — mount to /mnt, then type ''${BOLD}exit''${RESET}."
            echo ""
            bash || true
            TOTAL_STEPS=5
            ;;

        # ══════════════════════════════════════════
        # INSTALL ONLY
        # ══════════════════════════════════════════
        3)
            TOTAL_STEPS=5
            ;;

        *)
            fail "Invalid choice."
            exit 1
            ;;
        esac

        # ══════════════════════════════════════════
        # Install (shared by all modes)
        # ══════════════════════════════════════════
        header "$TOTAL_STEPS" "Installing"

        if ! mountpoint -q /mnt; then
            fail "/mnt is not mounted. Mount your filesystems first."
            exit 1
        fi

        echo "  ''${BOLD}Mounts:''${RESET}"
        findmnt --target /mnt --submounts --real -o TARGET,SOURCE,FSTYPE | head -20 | sed 's/^/  /'
        echo ""

        info "Generating hardware configuration..."
        nixos-generate-config --root /mnt
        success "Hardware detected."

        info "Copying repository to /mnt/os..."
        cp -a /tmp/bingux-os /mnt/os
        # Copy hardware-configuration.nix to machine dir if it exists
        if [[ -d "/mnt/os/machines/$TARGET_HOST" ]]; then
            cp /mnt/etc/nixos/hardware-configuration.nix \
                "/mnt/os/machines/$TARGET_HOST/hardware-configuration.nix"
        fi
        chown -R 1000:100 /mnt/os
        success "Flake ready."

        # Generate SSH host keys for sops-nix
        if [[ ! -f /mnt/etc/ssh/ssh_host_ed25519_key ]]; then
            info "Generating SSH host keys..."
            mkdir -p /mnt/etc/ssh
            ssh-keygen -t ed25519 -f /mnt/etc/ssh/ssh_host_ed25519_key -N "" -q
            success "SSH host keys generated."
            if command -v ssh-to-age &>/dev/null; then
                echo ""
                warn "To use sops-nix, add this age key to your .sops.yaml:"
                echo "    $(${pkgs.ssh-to-age}/bin/ssh-to-age < /mnt/etc/ssh/ssh_host_ed25519_key.pub)"
                echo ""
            fi
        fi

        # Set user password
        echo ""
        prompt_with_default "Username to set password for" "" INSTALL_USER
        if [[ -n "$INSTALL_USER" ]]; then
            info "Set password for user ''${BOLD}$INSTALL_USER''${RESET}:"
            while true; do
                read -s -p "  ''${BOLD}▸''${RESET} Password: " PASS1
                echo ""
                read -s -p "  ''${BOLD}▸''${RESET} Confirm:  " PASS2
                echo ""
                if [[ "$PASS1" == "$PASS2" ]] && [[ -n "$PASS1" ]]; then
                    break
                fi
                warn "Passwords don't match or are empty. Try again."
            done
        fi

        echo ""
        info "Installing Bingux ''${BOLD}$TARGET_HOST''${RESET}..."
        info "''${DIM}This may take a while.''${RESET}"
        echo ""

        INSTALL_START=$SECONDS

        nixos-install \
            --no-root-passwd \
            --root /mnt \
            --flake "/mnt/os#$TARGET_HOST"

        # Set user password in the installed system
        if [[ -n "''${INSTALL_USER:-}" ]] && [[ -n "''${PASS1:-}" ]]; then
            info "Setting user password..."
            echo "$INSTALL_USER:$PASS1" | nixos-enter --root /mnt -- chpasswd 2>/dev/null || \
                warn "Could not set password automatically. Set it after first boot with: passwd"
            unset PASS1 PASS2
        fi

        INSTALL_ELAPSED=$(( SECONDS - INSTALL_START ))

        # ── Done ──
        echo ""
        echo ""
        echo "  ''${GREEN}''${BOLD}┌──────────────────────────────────────┐''${RESET}"
        echo "  ''${GREEN}''${BOLD}│                                      │''${RESET}"
        echo "  ''${GREEN}''${BOLD}│     ✓  Installation Complete!        │''${RESET}"
        echo "  ''${GREEN}''${BOLD}│                                      │''${RESET}"
        echo "  ''${GREEN}''${BOLD}└──────────────────────────────────────┘''${RESET}"
        echo ""
        echo "  Installed ''${BOLD}$TARGET_HOST''${RESET} in ''${BOLD}$((INSTALL_ELAPSED / 60))m$((INSTALL_ELAPSED % 60))s''${RESET}"
        info "Log saved to: ''${DIM}$LOG_FILE''${RESET}"
        echo ""
        prompt "Do you want to reboot now? [y/N] " reboot_choice
        if [[ "$reboot_choice" =~ ^[Yy]$ ]]; then
            sudo reboot
        fi
    '';
in
{
    imports = [
        (modulesPath + "/installer/cd-dvd/installation-cd-graphical-base.nix")
        ./live-shell.nix
        ../system/branding.nix
    ];

    # GNOME + GDM autologin
    services.xserver.desktopManager.gnome.enable = lib.mkForce true;
    services.xserver.displayManager.gdm.enable = lib.mkForce true;
    services.displayManager.defaultSession = "gnome";
    services.displayManager.autoLogin = {
        enable = true;
        user = "bingux";
    };

    # Passwordless bingux user on the live installer
    users.users.bingux.hashedPassword = lib.mkForce "";
    security.sudo.wheelNeedsPassword = lib.mkForce false;

    # Plain dark background instead of NixOS wallpaper
    programs.dconf.profiles.user.databases = [{
        settings."org/gnome/desktop/background" = {
            picture-options = "none";
            primary-color = "#1a1a2e";
        };
        settings."org/gnome/desktop/screensaver" = {
            picture-options = "none";
            primary-color = "#1a1a2e";
        };
        settings."org/gnome/desktop/interface" = {
            color-scheme = "prefer-dark";
        };
        settings."org/gnome/shell" = {
            favorite-apps = [
                "bingux-installer.desktop"
                "org.gnome.Nautilus.desktop"
                "firefox.desktop"
                "org.gnome.Terminal.desktop"
            ];
        };
    }];

    environment.defaultPackages = lib.mkForce (with pkgs; [
        vim
        nano
    ]);

    environment.systemPackages = with pkgs; [
        firefox
        gh
        gparted
        gptfdisk
        ssh-to-age
        gnome-terminal
        gnome-text-editor
        bingux-install
        (makeDesktopItem {
            name = "bingux-installer";
            desktopName = "Install Bingux";
            comment = "Install Bingux to your disk";
            exec = "${gnome-terminal}/bin/gnome-terminal --geometry=100x40 -- sudo ${bingux-install}/bin/bingux-install";
            icon = "bingux";
            categories = [ "System" ];
        })
    ];

    # Locale and keyboard
    i18n.defaultLocale = lib.mkForce "en_GB.UTF-8";
    i18n.supportedLocales = lib.mkForce [
        "en_US.UTF-8/UTF-8"
        "en_GB.UTF-8/UTF-8"
    ];
    services.xserver.xkb.layout = lib.mkForce "gb";
    console.keyMap = lib.mkForce "uk";

    # Strip GNOME bloat from installer
    environment.gnome.excludePackages = with pkgs; [
        epiphany
        geary
        gnome-music
        gnome-photos
        gnome-software
        gnome-tour
        yelp
        gnome-maps
        gnome-contacts
        gnome-weather
        gnome-clocks
        gnome-calendar
        gnome-characters
        gnome-connections
        gnome-console
        gnome-logs
        gnome-system-monitor
        baobab
        simple-scan
        totem
        evince
        snapshot
        gnome-font-viewer
        gnome-disk-utility
    ];

    # Trim VM guest additions
    virtualisation.vmware.guest.enable = lib.mkForce false;

    # Disable nix-index in installer
    programs.nix-index.enable = lib.mkForce false;

    # Disable direnv in installer
    programs.direnv.enable = lib.mkForce false;

    # Exclude documentation
    documentation.enable = lib.mkForce false;
    documentation.man.enable = lib.mkForce false;
    documentation.info.enable = lib.mkForce false;
    documentation.doc.enable = lib.mkForce false;
    documentation.nixos.enable = lib.mkForce false;

    # Only include firmware for common hardware
    hardware.enableAllFirmware = lib.mkForce false;
    hardware.enableRedistributableFirmware = true;

    # Disable unneeded services
    services.printing.enable = lib.mkForce false;
    hardware.bluetooth.enable = lib.mkForce false;
    services.power-profiles-daemon.enable = lib.mkForce false;
    services.speechd.enable = lib.mkForce false;
    boot.swraid.enable = lib.mkForce false;
    virtualisation.hypervGuest.enable = lib.mkForce false;

    # Ensure virtio drivers are available
    boot.initrd.availableKernelModules = [
        "virtio_pci" "virtio_blk" "virtio_scsi" "virtio_net"
        "ahci" "sd_mod" "sr_mod" "usb_storage" "ehci_pci"
        "uhci_hcd" "xhci_pci" "nvme" "ata_piix"
    ];

    boot.supportedFilesystems = lib.mkForce [ "btrfs" "ext4" "xfs" "f2fs" "vfat" "ntfs3" ];

    nix.gc.automatic = true;
    nix.settings.auto-optimise-store = true;
    nix.settings.max-jobs = "auto";
    nix.settings.cores = 0;

    # Fonts
    fonts.packages = with pkgs; [
        adwaita-fonts
    ];

    # Plymouth
    boot.plymouth = {
        theme = lib.mkForce "bingux";
        themePackages = lib.mkForce [ bingux-plymouth ];
    };

    nix.settings.experimental-features = [ "nix-command" "flakes" ];
    system.nixos.distroName = "Bingux";
    isoImage.squashfsCompression = "zstd -Xcompression-level 22";
    isoImage.grubTheme = pkgs.minegrub-theme;
    isoImage.efiSplashImage = ../../files/branding/bingus.png;
    isoImage.splashImage = ../../files/branding/bingus-syslinux.png;
    isoImage.syslinuxTheme = ''
        MENU TITLE Bingux
        MENU RESOLUTION 800 600
        MENU CLEAR
        MENU ROWS 6
        MENU CMDLINEROW -4
        MENU TIMEOUTROW -3
        MENU TABMSGROW  -2
        MENU HELPMSGROW -1
        MENU HELPMSGENDROW -1
        MENU MARGIN 0

        MENU COLOR BORDER       30;44      #00000000    #00000000   none
        MENU COLOR SCREEN       37;40      #FF000000    #00E2E8FF   none
        MENU COLOR TABMSG       31;40      #80000000    #00000000   none
        MENU COLOR TIMEOUT      1;37;40    #FF000000    #00000000   none
        MENU COLOR TIMEOUT_MSG  37;40      #FF000000    #00000000   none
        MENU COLOR CMDMARK      1;36;40    #FF000000    #00000000   none
        MENU COLOR CMDLINE      37;40      #FF000000    #00000000   none
        MENU COLOR TITLE        1;36;44    #00000000    #00000000   none
        MENU COLOR UNSEL        37;44      #FF000000    #00000000   none
        MENU COLOR SEL          7;37;40    #FFFFFFFF    #FF5277C3   std
    '';

    networking.hostName = "bingux-installer";

    # Partition guide
    environment.etc."bingux-installer/PARTITION-GUIDE.txt".source =
        ../../files/installer/PARTITION-GUIDE.txt;

    # Installer .desktop file — shows in app launcher and autostart
    environment.etc."xdg/autostart/bingux-installer.desktop".text = ''
        [Desktop Entry]
        Type=Application
        Name=Install Bingux
        Comment=Install Bingux to your disk
        Exec=${pkgs.gnome-terminal}/bin/gnome-terminal --geometry=100x40 -- sudo ${bingux-install}/bin/bingux-install
        Icon=bingux
        Terminal=false
        Categories=System;
        X-GNOME-Autostart-Phase=Application
        X-GNOME-Autostart-Delay=2
    '';

    environment.etc."skel/Desktop/install-bingux.desktop".text = ''
        [Desktop Entry]
        Type=Application
        Name=Install Bingux
        Comment=Install Bingux to your disk
        Exec=${pkgs.gnome-terminal}/bin/gnome-terminal --geometry=100x40 -- sudo ${bingux-install}/bin/bingux-install
        Icon=bingux
        Terminal=false
        Categories=System;
    '';
}
