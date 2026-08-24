{
  inputs,
  system,
  pveTool,
}:
let
  pkgs = inputs.nixpkgs.legacyPackages.${system};
in
pkgs.runCommand "bingux-pve-development-tool-check"
  {
    nativeBuildInputs = [
      pveTool
      pkgs.coreutils
      pkgs.gnugrep
      pkgs.gnused
    ];
  }
  ''
    create_output=$(env \
        PVE_API_URL=https://pve.example:8006/api2/json \
        PVE_API_TOKEN_ID=user@pam!bingux \
        PVE_API_TOKEN_FILE=/dev/null \
        PVE_NODE=pve \
        PVE_ISO_STORAGE=iso-store \
        PVE_VM_STORAGE=vm-store \
        PVE_BRIDGE=vmbr0 \
        bingux-pve create \
        --iso /tmp/bingux.iso \
        --evidence-dir /tmp/bingux-evidence \
        --dry-run)
    printf '%s\n' "$create_output" | grep -Fqx \
        'DRY-RUN POST https://pve.example:8006/api2/json/nodes/pve/storage/iso-store/upload'
    printf '%s\n' "$create_output" | grep -Fqx \
        'DRY-RUN FORM content=iso filename=bingux.iso'
    printf '%s\n' "$create_output" | grep -Fqx \
        'DRY-RUN DATA scsi0=vm-store:64,discard=on'
    printf '%s\n' "$create_output" | grep -Fqx \
        'DRY-RUN DATA ide2=iso-store:iso/bingux.iso,media=cdrom'
    printf '%s\n' "$create_output" | grep -Fqx \
        'DRY-RUN DATA boot=order=ide2;scsi0'
    printf '%s\n' "$create_output" | grep -Fqx \
        'DRY-RUN DATA vmid=<vmid>'
    printf '%s\n' "$create_output" | grep -Fqx \
        'DRY-RUN DATA name=bingux-install-<vmid>'
    printf '%s\n' "$create_output" | grep -Fqx \
        'DRY-RUN DATA cores=8'
    printf '%s\n' "$create_output" | grep -Fqx \
        'DRY-RUN DATA memory=8192'
    printf '%s\n' "$create_output" | grep -Fqx \
        'DRY-RUN DATA cpu=host'
    printf '%s\n' "$create_output" | grep -Fqx \
        'DRY-RUN DATA ostype=l26'
    printf '%s\n' "$create_output" | grep -Fqx \
        'DRY-RUN DATA net0=virtio,bridge=vmbr0'
    printf '%s\n' "$create_output" | grep -Fqx \
        'DRY-RUN DATA tags=bingux-pve-test'

    custom_create_output=$(env \
        PVE_API_URL=https://pve.example:8006/api2/json \
        PVE_API_TOKEN_ID=user@pam!bingux \
        PVE_API_TOKEN_FILE=/dev/null \
        PVE_NODE=pve \
        PVE_ISO_STORAGE=iso-store \
        PVE_VM_STORAGE=vm-store \
        PVE_BRIDGE=vmbr0 \
        PVE_VM_CORES=4 \
        PVE_VM_MEMORY_MIB=6144 \
        bingux-pve create \
        --iso /tmp/bingux.iso \
        --evidence-dir /tmp/bingux-evidence \
        --dry-run)
    printf '%s\n' "$custom_create_output" | grep -Fqx \
        'DRY-RUN DATA cores=4'
    printf '%s\n' "$custom_create_output" | grep -Fqx \
        'DRY-RUN DATA memory=6144'

    if env \
        PVE_API_URL=https://pve.example:8006/api2/json \
        PVE_API_TOKEN_ID=user@pam!bingux \
        PVE_API_TOKEN_FILE=/dev/null \
        PVE_NODE=pve \
        PVE_ISO_STORAGE=iso-store \
        PVE_VM_STORAGE=vm-store \
        PVE_BRIDGE=vmbr0 \
        PVE_VM_CORES=0 \
        bingux-pve create \
        --iso /tmp/bingux.iso \
        --evidence-dir /tmp/bingux-evidence \
        --dry-run >/dev/null 2>&1; then
        printf '%s\n' 'create accepted an out-of-range VM vCPU count' >&2
        exit 1
    fi
    if env \
        PVE_API_URL=https://pve.example:8006/api2/json \
        PVE_API_TOKEN_ID=user@pam!bingux \
        PVE_API_TOKEN_FILE=/dev/null \
        PVE_NODE=pve \
        PVE_ISO_STORAGE=iso-store \
        PVE_VM_STORAGE=vm-store \
        PVE_BRIDGE=vmbr0 \
        PVE_VM_MEMORY_MIB=511 \
        bingux-pve create \
        --iso /tmp/bingux.iso \
        --evidence-dir /tmp/bingux-evidence \
        --dry-run >/dev/null 2>&1; then
        printf '%s\n' 'create accepted out-of-range VM memory' >&2
        exit 1
    fi


    upid_validator=$(sed -n '/^validate_upid() {/,/^}/p' "$(command -v bingux-pve)")
    if ! bash -c "$upid_validator"$'\nvalidate_upid "$1"' _ \
        'UPID:pve:00000000:00000000:00000000:imgcopy::root@pam:'; then
        printf '%s\n' 'validator rejected an empty-id imgcopy UPID' >&2
        exit 1
    fi

    if bash -c "$upid_validator"$'\nvalidate_upid "$1"' _ \
        'UPID:pve:00000000:00000000:00000000:imgcopy::root/pam:'; then
        printf '%s\n' 'validator accepted an invalid imgcopy UPID user' >&2
        exit 1
    fi

    printf '%s\n' "$create_output" | grep -Fqx \
        'DRY-RUN POST https://pve.example:8006/api2/json/nodes/pve/qemu/<vmid>/status/start'

    if env \
        PVE_API_URL=https://pve.example:8006/api2/json \
        PVE_API_TOKEN_ID=user@pam!bingux \
        PVE_API_TOKEN_FILE=/dev/null \
        PVE_NODE=pve \
        PVE_ISO_STORAGE=iso-store \
        PVE_BRIDGE=vmbr0 \
        bingux-pve create \
        --iso /tmp/bingux.iso \
        --evidence-dir /tmp/bingux-evidence \
        --dry-run >/dev/null 2>&1; then
        printf '%s\n' 'create accepted a missing VM storage variable' >&2
        exit 1
    fi
    if env \
        PVE_API_URL=https://pve.example:8006/api2/html \
        PVE_API_TOKEN_ID=user@pam!bingux \
        PVE_API_TOKEN_FILE=/dev/null \
        PVE_NODE=pve \
        PVE_ISO_STORAGE=iso-store \
        PVE_VM_STORAGE=vm-store \
        PVE_BRIDGE=vmbr0 \
        bingux-pve create \
        --iso /tmp/bingux.iso \
        --evidence-dir /tmp/bingux-evidence \
        --dry-run >/dev/null 2>&1; then
        printf '%s\n' 'create accepted a non-API JSON Proxmox URL' >&2
        exit 1
    fi
    bad_token_file="$TMPDIR/bad-token"
    iso_file="$TMPDIR/bingux.iso"
    printf '%s\n' 'test-token' >"$bad_token_file"
    printf '%s\n' 'test ISO content' >"$iso_file"
    chmod 0644 "$bad_token_file"
    if env \
        PVE_API_URL=https://pve.example:8006/api2/json \
        PVE_API_TOKEN_ID=user@pam!bingux \
        PVE_API_TOKEN_FILE="$bad_token_file" \
        PVE_NODE=pve \
        PVE_ISO_STORAGE=iso-store \
        PVE_VM_STORAGE=vm-store \
        PVE_BRIDGE=vmbr0 \
        bingux-pve create \
        --iso "$iso_file" \
        --evidence-dir "$TMPDIR/bingux-evidence" >/dev/null 2>&1; then
        printf '%s\n' 'create accepted a group-readable API token file' >&2
        exit 1
    fi
    multiple_line_token_file="$TMPDIR/multiple-line-token"
    printf 'test-token\n\n' >"$multiple_line_token_file"
    chmod 0600 "$multiple_line_token_file"
    if env \
        PVE_API_URL=https://pve.example:8006/api2/json \
        PVE_API_TOKEN_ID=user@pam!bingux \
        PVE_API_TOKEN_FILE="$multiple_line_token_file" \
        PVE_NODE=pve \
        PVE_ISO_STORAGE=iso-store \
        PVE_VM_STORAGE=vm-store \
        PVE_BRIDGE=vmbr0 \
        bingux-pve create \
        --iso "$iso_file" \
        --evidence-dir "$TMPDIR/bingux-evidence" >/dev/null 2>&1; then
        printf '%s\n' 'create accepted a multi-line API token file' >&2
        exit 1
    fi

    BINGUX_PVE_SOURCE_ONLY=1 bash -c '
        source "$(command -v bingux-pve)"

        workspace=$(mktemp -d)
        PVE_API_TOKEN_FILE="$workspace/token"
        PVE_API_TOKEN_ID="user@pam!bingux"
        EVIDENCE_DIR="$workspace/evidence"
        printf "%s\n" "first-token" >"$PVE_API_TOKEN_FILE"
        chmod 0600 "$PVE_API_TOKEN_FILE"
        prepare_runtime
        printf "%s\n" "replacement-token" >"$PVE_API_TOKEN_FILE"
        printf "%s\n" "[{\"n\":1,\"t\":\"first-token user@pam!bingux\"}]" >"$TMP_DIR/task-log.json"
        if ! redact_task_log "$TMP_DIR/task-log.json" "$TMP_DIR/redacted-log.json"; then
            printf "%s\n" "task-log redaction failed after token file replacement" >&2
            exit 1
        fi
        if grep -Fq "first-token" "$TMP_DIR/redacted-log.json"; then
            printf "%s\n" "task-log redaction leaked the captured API token" >&2
            exit 1
        fi
        if grep -Fq "user@pam!bingux" "$TMP_DIR/redacted-log.json"; then
            printf "%s\n" "task-log redaction leaked the API token identity" >&2
            exit 1
        fi
        rm -rf -- "$workspace"
    '


    BINGUX_PVE_SOURCE_ONLY=1 bash -c '
        source "$(command -v bingux-pve)"

        TMP_DIR=$(mktemp -d)
        BASE_URL="https://pve.example:8006/api2/json"
        PVE_NODE="pve"
        VMID=100
        RUN_DIR="$TMP_DIR/evidence"
        EVIDENCE_SEQUENCE=0
        call_log="$TMP_DIR/calls"
        mkdir -p "$RUN_DIR"

        api_request() {
            local operation=$1
            local method=$2
            local endpoint=$3
            local response_file=$4
            printf "%s %s\n" "$method" "$endpoint" >>"$call_log"
            if [[ "$method" == "GET" && "$endpoint" == "$BASE_URL/nodes/$PVE_NODE/qemu/$VMID/config" ]]; then
                printf "%s\n" "{\"data\":{\"name\":\"bingux-install-$VMID\",\"tags\":\"bingux-pve-test\"}}" >"$response_file"
                return 0
            fi
            if [[ "$method" == "GET" && "$endpoint" == "$BASE_URL/nodes/$PVE_NODE/qemu/$VMID/status/current" ]]; then
                printf "%s\n" "{\"data\":{\"status\":\"stopped\"}}" >"$response_file"
                return 0
            fi
            if [[ "$method" == "DELETE" && "$endpoint" == "$BASE_URL/nodes/$PVE_NODE/qemu/$VMID?purge=1&destroy-unreferenced-disks=1" ]]; then
                printf "%s\n" "{\"data\":\"UPID:pve:00000000:00000000:00000000:qmdestroy:100:root@pam:\"}" >"$response_file"
                return 0
            fi
            return 1
        }

        write_request_evidence() {
            return 0
        }

        wait_for_task() {
            return 0
        }

        if ! destroy_vm "destroy"; then
            printf "%s\n" "destroy failed for an already stopped VM" >&2
            exit 1
        fi
        grep -Fqx "GET $BASE_URL/nodes/$PVE_NODE/qemu/$VMID/config" "$call_log"
        grep -Fqx "GET $BASE_URL/nodes/$PVE_NODE/qemu/$VMID/status/current" "$call_log"
        grep -Fqx "DELETE $BASE_URL/nodes/$PVE_NODE/qemu/$VMID?purge=1&destroy-unreferenced-disks=1" "$call_log"
        if grep -Fq "/status/shutdown" "$call_log" || grep -Fq "/status/stop" "$call_log"; then
            printf "%s\n" "destroy sent a power command for an already stopped VM" >&2
            exit 1
        fi
        rm -rf -- "$TMP_DIR"
    '

    BINGUX_PVE_SOURCE_ONLY=1 bash -c '
        source "$(command -v bingux-pve)"

        TMP_DIR=$(mktemp -d)
        BASE_URL="https://pve.example:8006/api2/json"
        PVE_NODE="pve"
        VMID=101
        RUN_DIR="$TMP_DIR/evidence"
        EVIDENCE_SEQUENCE=0
        call_log="$TMP_DIR/calls"
        mkdir -p "$RUN_DIR"

        api_request() {
            local operation=$1
            local method=$2
            local endpoint=$3
            local response_file=$4
            printf "%s %s\n" "$method" "$endpoint" >>"$call_log"
            if [[ "$method" == "GET" && "$endpoint" == "$BASE_URL/nodes/$PVE_NODE/qemu/$VMID/config" ]]; then
                printf "%s\n" "{\"data\":{\"name\":\"bingux-install-$VMID\",\"tags\":\"other\"}}" >"$response_file"
                return 0
            fi
            return 1
        }

        write_request_evidence() {
            return 0
        }

        if destroy_vm "destroy"; then
            printf "%s\n" "destroy accepted an unowned VM" >&2
            exit 1
        fi
        grep -Fqx "GET $BASE_URL/nodes/$PVE_NODE/qemu/$VMID/config" "$call_log"
        if grep -Fq "DELETE " "$call_log"; then
            printf "%s\n" "destroy deleted an unowned VM" >&2
            exit 1
        fi
        rm -rf -- "$TMP_DIR"
    '

    BINGUX_PVE_SOURCE_ONLY=1 bash -c '
        source "$(command -v bingux-pve)"

        VMID=102
        VM_CREATION_ATTEMPTED=1
        DESTROY_ON_FAILURE=1
        CLEANUP_ATTEMPTED=0
        LAST_EVIDENCE_WRITTEN=1
        CREATE_TASK_TERMINAL=1
        call_log=$(mktemp)

        destroy_vm() {
            printf "%s\n" "$1" >"$call_log"
        }

        maybe_cleanup_after_failure
        grep -Fqx "failure-cleanup" "$call_log"
        rm -f -- "$call_log"
    '

    BINGUX_PVE_SOURCE_ONLY=1 bash -c '
        source "$(command -v bingux-pve)"

        TMP_DIR=$(mktemp -d)
        BASE_URL="https://pve.example:8006/api2/json"
        PVE_NODE="pve"
        PVE_ISO_STORAGE="iso-store"
        PVE_VM_STORAGE="vm-store"
        PVE_BRIDGE="vmbr0"
        PVE_VM_CORES=8
        PVE_VM_MEMORY_MIB=8192
        ISO="/tmp/bingux.iso"
        ISO_NAME="bingux.iso"
        VMID=103
        RUN_DIR="$TMP_DIR/evidence"
        EVIDENCE_SEQUENCE=0
        VM_CREATION_ATTEMPTED=0
        CREATE_TASK_TERMINAL=0
        DESTROY_ON_FAILURE=1
        CLEANUP_ATTEMPTED=0
        LAST_EVIDENCE_WRITTEN=0
        call_log="$TMP_DIR/calls"
        mkdir -p "$RUN_DIR"

        api_request() {
            local operation=$1
            local method=$2
            local endpoint=$3
            local response_file=$4
            printf "%s %s %s\n" "$operation" "$method" "$endpoint" >>"$call_log"
            case "$operation" in
                upload)
                    printf "%s\n" "{\"data\":\"UPID:pve:00000000:00000000:00000000:imgcopy::root@pam:\"}" >"$response_file"
                    ;;
                nextid)
                    printf "%s\n" "{\"data\":$VMID}" >"$response_file"
                    ;;
                create)
                    printf "%s\n" "{\"data\":\"UPID:pve:00000000:00000000:00000000:qmcreate:$VMID:root@pam:\"}" >"$response_file"
                    ;;
                failure-cleanup.ownership)
                    printf "%s\n" "{\"data\":{\"name\":\"bingux-install-$VMID\",\"tags\":\"bingux-pve-test\"}}" >"$response_file"
                    ;;
                failure-cleanup.status)
                    printf "%s\n" "{\"data\":{\"status\":\"stopped\"}}" >"$response_file"
                    ;;
                failure-cleanup)
                    printf "%s\n" "{\"data\":\"UPID:pve:00000000:00000000:00000000:qmdestroy:$VMID:root@pam:\"}" >"$response_file"
                    ;;
                *)
                    return 1
                    ;;
            esac
        }

        write_request_evidence() {
            return 0
        }

        wait_for_task() {
            LAST_EVIDENCE_WRITTEN=1
            if [[ "$1" == create ]]; then
                LAST_TASK_TERMINAL=1
                return 1
            fi
            LAST_TASK_TERMINAL=1
        }

        if create_vm; then
            printf "%s\n" "create accepted an unsuccessful create task" >&2
            exit 1
        fi
        maybe_cleanup_after_failure
        grep -Fqx "failure-cleanup.ownership GET $BASE_URL/nodes/$PVE_NODE/qemu/$VMID/config" "$call_log"
        grep -Fqx "failure-cleanup DELETE $BASE_URL/nodes/$PVE_NODE/qemu/$VMID?purge=1&destroy-unreferenced-disks=1" "$call_log"
        rm -rf -- "$TMP_DIR"
    '

    BINGUX_PVE_SOURCE_ONLY=1 bash -c '
        source "$(command -v bingux-pve)"

        VMID=104
        VM_CREATION_ATTEMPTED=1
        CREATE_TASK_TERMINAL=0
        DESTROY_ON_FAILURE=1
        CLEANUP_ATTEMPTED=0
        LAST_EVIDENCE_WRITTEN=1
        call_log=$(mktemp)

        destroy_vm() {
            printf "%s\n" "$1" >"$call_log"
        }

        maybe_cleanup_after_failure
        if [[ -s $call_log ]]; then
            printf "%s\n" "failure cleanup ran before the create task was terminal" >&2
            exit 1
        fi
        rm -f -- "$call_log"
    '

    BINGUX_PVE_SOURCE_ONLY=1 bash -c '
        source "$(command -v bingux-pve)"

        TMP_DIR=$(mktemp -d)
        BASE_URL="https://pve.example:8006/api2/json"
        PVE_NODE="pve"

        api_request() {
            local operation=$1
            local method=$2
            local endpoint=$3
            local response_file=$4
            if [[ "$operation" == start.poll.status ]]; then
                printf "%s\n" "{\"data\":{\"status\":\"stopped\",\"exitstatus\":\"OK\"}}" >"$response_file"
                return 0
            fi
            return 1
        }

        write_task_evidence() {
            return 0
        }

        if ! wait_for_task "start" "$BASE_URL/nodes/$PVE_NODE/qemu/100/status/start" \
            "UPID:pve:00000000:00000000:00000000:qmstart:100:root@pam:" "$TMP_DIR/evidence.json"; then
            printf "%s\n" "task log failure changed a successful task into a failed task" >&2
            exit 1
        fi
        if (( LAST_TASK_TERMINAL != 1 || LAST_EVIDENCE_WRITTEN != 1 )); then
            printf "%s\n" "successful task did not retain terminal and evidence state" >&2
            exit 1
        fi
        rm -rf -- "$TMP_DIR"
    '


    BINGUX_PVE_SOURCE_ONLY=1 bash -c '
        source "$(command -v bingux-pve)"

        TMP_DIR=$(mktemp -d)
        BASE_URL="https://pve.example:8006/api2/json"
        PVE_NODE="pve"
        VMID=107
        RUN_DIR="$TMP_DIR/evidence"
        EVIDENCE_SEQUENCE=0
        VM_CREATION_ATTEMPTED=1
        CREATE_TASK_TERMINAL=1
        START_REQUEST_ATTEMPTED=0
        START_TASK_TERMINAL=0
        DESTROY_ON_FAILURE=1
        CLEANUP_ATTEMPTED=0
        LAST_EVIDENCE_WRITTEN=1
        call_log="$TMP_DIR/calls"
        mkdir -p "$RUN_DIR"

        api_request() {
            local operation=$1
            local response_file=$4
            case "$operation" in
                start)
                    printf "%s\n" "{\"data\":\"UPID:pve:00000000:00000000:00000000:qmstart:$VMID:root@pam:\"}" >"$response_file"
                    return 0
                    ;;
                start.poll.status)
                    printf "%s\n" "{\"data\":{\"status\":\"stopped\"}}" >"$response_file"
                    return 0
                    ;;
                *)
                    return 1
                    ;;
            esac
        }

        write_task_evidence() {
            return 0
        }

        destroy_vm() {
            printf "%s\n" "$1" >"$call_log"
        }

        if start_vm; then
            printf "%s\n" "start accepted a task without an exit status" >&2
            exit 1
        fi
        if (( START_REQUEST_ATTEMPTED != 1 || START_TASK_TERMINAL != 0 || LAST_TASK_TERMINAL != 0 || LAST_EVIDENCE_WRITTEN != 1 )); then
            printf "%s\n" "start did not retain the unknown task outcome" >&2
            exit 1
        fi
        maybe_cleanup_after_failure
        if [[ -s $call_log ]]; then
            printf "%s\n" "failure cleanup ran after a task without an exit status" >&2
            exit 1
        fi
        rm -rf -- "$TMP_DIR"
    '

    BINGUX_PVE_SOURCE_ONLY=1 bash -c '
        source "$(command -v bingux-pve)"

        TMP_DIR=$(mktemp -d)
        BASE_URL="https://pve.example:8006/api2/json"
        PVE_NODE="pve"
        VMID=105
        RUN_DIR="$TMP_DIR/evidence"
        EVIDENCE_SEQUENCE=0
        VM_CREATION_ATTEMPTED=1
        CREATE_TASK_TERMINAL=1
        START_REQUEST_ATTEMPTED=0
        START_TASK_TERMINAL=0
        DESTROY_ON_FAILURE=1
        CLEANUP_ATTEMPTED=0
        LAST_EVIDENCE_WRITTEN=1
        call_log="$TMP_DIR/calls"
        mkdir -p "$RUN_DIR"

        api_request() {
            return 1
        }

        write_request_evidence() {
            return 0
        }

        destroy_vm() {
            printf "%s\n" "$1" >"$call_log"
        }

        if start_vm; then
            printf "%s\n" "start accepted an unavailable start request" >&2
            exit 1
        fi
        if (( START_REQUEST_ATTEMPTED != 1 || START_TASK_TERMINAL != 0 )); then
            printf "%s\n" "start did not retain the ambiguous request state" >&2
            exit 1
        fi
        maybe_cleanup_after_failure
        if [[ -s $call_log ]]; then
            printf "%s\n" "failure cleanup ran after an ambiguous start request" >&2
            exit 1
        fi
        rm -rf -- "$TMP_DIR"
    '

    BINGUX_PVE_SOURCE_ONLY=1 bash -c '
        source "$(command -v bingux-pve)"

        TMP_DIR=$(mktemp -d)
        BASE_URL="https://pve.example:8006/api2/json"
        PVE_NODE="pve"
        PVE_ISO_STORAGE="iso-store"
        PVE_VM_STORAGE="vm-store"
        PVE_BRIDGE="vmbr0"
        PVE_VM_CORES=8
        PVE_VM_MEMORY_MIB=8192
        ISO="/tmp/bingux.iso"
        ISO_NAME="bingux.iso"
        VMID=106
        RUN_DIR="$TMP_DIR/evidence"
        EVIDENCE_SEQUENCE=0
        DESTROY_ON_FAILURE=1
        CLEANUP_ATTEMPTED=0
        LAST_EVIDENCE_WRITTEN=0
        call_log="$TMP_DIR/calls"
        mkdir -p "$RUN_DIR"

        api_request() {
            local operation=$1
            local response_file=$4
            printf "%s\n" "$operation" >>"$call_log"
            case "$operation" in
                upload)
                    printf "%s\n" "{\"data\":\"UPID:pve:00000000:00000000:00000000:imgcopy::root@pam:\"}" >"$response_file"
                    ;;
                nextid)
                    printf "%s\n" "{\"data\":$VMID}" >"$response_file"
                    ;;
                create)
                    printf "%s\n" "{\"data\":\"UPID:pve:00000000:00000000:00000000:qmcreate:$VMID:root@pam:\"}" >"$response_file"
                    ;;
                start)
                    printf "%s\n" "{\"data\":\"UPID:pve:00000000:00000000:00000000:qmstart:$VMID:root@pam:\"}" >"$response_file"
                    ;;
                *)
                    return 1
                    ;;
            esac
        }

        wait_for_task() {
            LAST_EVIDENCE_WRITTEN=1
            case "$1" in
                upload|create)
                    LAST_TASK_TERMINAL=1
                    return 0
                    ;;
                start)
                    LAST_TASK_TERMINAL=0
                    return 1
                    ;;
                *)
                    return 1
                    ;;
            esac
        }

        destroy_vm() {
            printf "%s\n" "$1" >"$TMP_DIR/destroy"
        }

        if create_vm; then
            printf "%s\n" "create accepted a nonterminal start task" >&2
            exit 1
        fi
        if (( CREATE_TASK_TERMINAL != 1 || START_REQUEST_ATTEMPTED != 1 || START_TASK_TERMINAL != 0 )); then
            printf "%s\n" "create did not preserve start task lifecycle state" >&2
            exit 1
        fi
        maybe_cleanup_after_failure
        if [[ -s $TMP_DIR/destroy ]]; then
            printf "%s\n" "failure cleanup ran before the start task was terminal" >&2
            exit 1
        fi
        rm -rf -- "$TMP_DIR"
    '

    BINGUX_PVE_SOURCE_ONLY=1 bash -c '
        source "$(command -v bingux-pve)"

        TMP_DIR=$(mktemp -d)
        PVE_CURL_CONFIG="$TMP_DIR/curl.conf"
        CURL_CA_ARGS=()
        test_bin="$TMP_DIR/bin"
        args_file="$TMP_DIR/curl-args"
        mkdir -p "$test_bin"
        cat >"$test_bin/curl" <<EOF
#!${pkgs.runtimeShell}
set -euo pipefail
printf "%s\n" "\$@" >"\$BINGUX_TEST_CURL_ARGS"
output_file=
while (( \$# > 0 )); do
    case "\$1" in
        --output)
            output_file="\$2"
            shift 2
            ;;
        *)
            shift
            ;;
    esac
done
printf "%s\n" "{\"data\":{}}" >"\$output_file"
printf "200"
EOF
        chmod 0700 "$test_bin/curl"
        export BINGUX_TEST_CURL_ARGS="$args_file"
        export PATH="$test_bin:$PATH"

        if ! api_request_with_timeout 7 bounded GET "https://pve.example:8006/api2/json/version" "$TMP_DIR/response.json"; then
            printf "%s\n" "curl error output:" >&2
            cat "$TMP_DIR/curl-error" >&2 || true
            printf "%s\n" "captured curl arguments:" >&2
            cat "$args_file" >&2 || true
            printf "%s\n" "bounded API request failed" >&2
            exit 1
        fi
        first_arg=$(sed -n "1p" "$args_file")
        if [[ $first_arg != --disable ]]; then
            printf "%s\n" "API request did not disable user curl configuration first" >&2
            exit 1
        fi
        for expected_arg in --connect-timeout 10 --max-time 7 --max-filesize 1048576 --speed-limit 1024 --speed-time 30; do
            if ! grep -Fqx -- "$expected_arg" "$args_file"; then
                printf "%s\n" "API request omitted required curl argument $expected_arg" >&2
                exit 1
            fi
        done
        rm -rf -- "$TMP_DIR"
    '

    cleanup_dry_run=$(env \
        PVE_API_URL=https://pve.example:8006/api2/json \
        PVE_API_TOKEN_ID=user@pam!bingux \
        PVE_API_TOKEN_FILE=/dev/null \
        PVE_NODE=pve \
        PVE_ISO_STORAGE=iso-store \
        PVE_VM_STORAGE=vm-store \
        PVE_BRIDGE=vmbr0 \
        bingux-pve create \
        --iso /tmp/bingux.iso \
        --evidence-dir /tmp/bingux-evidence \
        --destroy-on-failure \
        --dry-run)
    printf "%s\n" "$cleanup_dry_run" | grep -Fqx \
        "DRY-RUN GET https://pve.example:8006/api2/json/nodes/pve/qemu/<vmid>/config"
    printf "%s\n" "$cleanup_dry_run" | grep -Fqx \
        "DRY-RUN GET https://pve.example:8006/api2/json/nodes/pve/qemu/<vmid>/status/current"
    printf "%s\n" "$cleanup_dry_run" | grep -Fqx \
        "DRY-RUN POST https://pve.example:8006/api2/json/nodes/pve/qemu/<vmid>/status/shutdown"
    printf "%s\n" "$cleanup_dry_run" | grep -Fqx \
        "DRY-RUN POST https://pve.example:8006/api2/json/nodes/pve/qemu/<vmid>/status/stop"
    printf "%s\n" "$cleanup_dry_run" | grep -Fqx \
        "DRY-RUN DELETE https://pve.example:8006/api2/json/nodes/pve/qemu/<vmid>?purge=1&destroy-unreferenced-disks=1"
    cleanup_config_line=$(printf "%s\n" "$cleanup_dry_run" | grep -nF \
        "/nodes/pve/qemu/<vmid>/config" | cut -d: -f1)
    cleanup_status_line=$(printf "%s\n" "$cleanup_dry_run" | grep -nF \
        "/nodes/pve/qemu/<vmid>/status/current" | cut -d: -f1)
    cleanup_shutdown_line=$(printf "%s\n" "$cleanup_dry_run" | grep -nF \
        "/nodes/pve/qemu/<vmid>/status/shutdown" | cut -d: -f1)
    cleanup_stop_line=$(printf "%s\n" "$cleanup_dry_run" | grep -nF \
        "/nodes/pve/qemu/<vmid>/status/stop" | cut -d: -f1)
    cleanup_delete_line=$(printf "%s\n" "$cleanup_dry_run" | grep -nF \
        "DRY-RUN DELETE https://pve.example:8006/api2/json/nodes/pve/qemu/<vmid>?" | cut -d: -f1)
    if (( cleanup_config_line >= cleanup_status_line || cleanup_status_line >= cleanup_shutdown_line || cleanup_shutdown_line >= cleanup_stop_line || cleanup_stop_line >= cleanup_delete_line )); then
        printf "%s\n" "create cleanup dry-run omitted ownership -> state -> shutdown -> stop -> delete order" >&2
        exit 1
    fi
    destroy_output=$(env \
        PVE_API_URL=https://pve.example:8006/api2/json \
        PVE_API_TOKEN_ID=user@pam!bingux \
        PVE_API_TOKEN_FILE=/dev/null \
        PVE_NODE=pve \
        bingux-pve destroy \
        --vmid 100 \
        --evidence-dir /tmp/bingux-evidence \
        --dry-run)
    printf '%s\n' "$destroy_output" | grep -Fqx \
        'DRY-RUN GET https://pve.example:8006/api2/json/nodes/pve/qemu/100/config'
    printf '%s\n' "$destroy_output" | grep -Fqx \
        'DRY-RUN GET https://pve.example:8006/api2/json/nodes/pve/qemu/100/status/current'
    printf '%s\n' "$destroy_output" | grep -Fqx \
        'DRY-RUN POST https://pve.example:8006/api2/json/nodes/pve/qemu/100/status/shutdown'
    printf '%s\n' "$destroy_output" | grep -Fqx \
        'DRY-RUN POST https://pve.example:8006/api2/json/nodes/pve/qemu/100/status/stop'
    printf '%s\n' "$destroy_output" | grep -Fqx \
        'DRY-RUN DELETE https://pve.example:8006/api2/json/nodes/pve/qemu/100?purge=1&destroy-unreferenced-disks=1'
    config_line=$(printf '%s\n' "$destroy_output" | grep -nF \
        '/nodes/pve/qemu/100/config' | cut -d: -f1)
    status_line=$(printf '%s\n' "$destroy_output" | grep -nF \
        '/nodes/pve/qemu/100/status/current' | cut -d: -f1)
    shutdown_line=$(printf '%s\n' "$destroy_output" | grep -nF \
        '/nodes/pve/qemu/100/status/shutdown' | cut -d: -f1)
    stop_line=$(printf '%s\n' "$destroy_output" | grep -nF \
        '/nodes/pve/qemu/100/status/stop' | cut -d: -f1)
    delete_line=$(printf '%s\n' "$destroy_output" | grep -nF \
        'DRY-RUN DELETE https://pve.example:8006/api2/json/nodes/pve/qemu/100?' | cut -d: -f1)
    if (( config_line >= status_line || status_line >= shutdown_line || shutdown_line >= stop_line || stop_line >= delete_line )); then
        printf '%s\n' 'destroy dry-run omitted config check -> state probe -> shutdown -> stop -> delete order' >&2
        exit 1
    fi
    touch "$out"
  ''
