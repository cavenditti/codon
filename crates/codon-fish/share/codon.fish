# codon fish-shell integration — MVP `#@` command-completion flow.
#
# Source from `~/.config/fish/conf.d/codon.fish`. When `CODON_SOCK`
# is unset (running outside codon, or codon-fish failed to bind),
# this file is a no-op — the keybinding is not installed and `codon`
# is not defined.
#
# Trigger: `Ctrl-G` reads the current commandline, splits on the
# LAST `#@`, sends `{partial, description, cwd, shell}` to the
# codon agent over `$CODON_SOCK`, and replaces the buffer with the
# returned command. Enter remains yours — never auto-executed.

if not set -q CODON_SOCK
    exit 0
end

# A second, idempotent guard so the plugin can be sourced twice in
# the same shell without redefining the binding.
if set -q __codon_fish_loaded
    exit 0
end
set -g __codon_fish_loaded 1

# JSON-line RPC over the Unix socket. We prefer the codon binary's
# own `rpc` helper when present (future work); for now use whichever
# of socat / ncat / nc is available. `python3` is the last-resort
# fallback because it ships with every modern distro and macOS.
function __codon_rpc --description 'send one JSON-line RPC to codon'
    set -l payload $argv[1]
    if type -q socat
        printf '%s\n' $payload | socat - UNIX-CONNECT:$CODON_SOCK
        return $status
    end
    if type -q ncat
        printf '%s\n' $payload | ncat -U $CODON_SOCK
        return $status
    end
    # macOS nc supports -U; some Linux distros ship ncat-style nc.
    if type -q nc
        printf '%s\n' $payload | nc -U -N $CODON_SOCK 2>/dev/null
        if test $status -eq 0
            return 0
        end
        # -N missing on macOS nc; retry without it.
        printf '%s\n' $payload | nc -U $CODON_SOCK
        return $status
    end
    if type -q python3
        python3 -c '
import json, socket, sys
sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
sock.connect(sys.argv[1])
sock.sendall((sys.argv[2] + "\n").encode("utf-8"))
buf = b""
while True:
    chunk = sock.recv(4096)
    if not chunk:
        break
    buf += chunk
    if b"\n" in buf:
        break
sys.stdout.write(buf.decode("utf-8", "replace"))
' $CODON_SOCK $payload
        return $status
    end
    echo 'codon: no socat / ncat / nc / python3 available for #@ RPC' >&2
    return 1
end

# Parse the buffer on the LAST `#@`. Earlier `#@` occurrences stay
# part of the partial (they'll be shell comments at execution time).
# Sets global `$__codon_partial` / `$__codon_description`.
function __codon_parse_hash_at
    set -l buffer $argv[1]
    set -l parts (string split '#@' -- $buffer)
    if test (count $parts) -le 1
        set -g __codon_partial ''
        set -g __codon_description (string trim -- $buffer)
        return
    end
    set -g __codon_partial (string trim -- (string join '#@' $parts[1..-2]))
    set -g __codon_description (string trim -- $parts[-1])
end

# Minimal JSON-string escape: backslash, double quote, newline,
# carriage return, tab. Enough for the fields we send.
function __codon_json_escape
    set -l s $argv[1]
    set s (string replace -a '\\' '\\\\' -- $s)
    set s (string replace -a '"' '\\"' -- $s)
    set s (string replace -a \n '\\n' -- $s)
    set s (string replace -a \r '\\r' -- $s)
    set s (string replace -a \t '\\t' -- $s)
    echo -n $s
end

function __codon_hash_at_trigger
    set -l original (commandline -b)
    if test -z "$original"
        return
    end
    __codon_parse_hash_at "$original"
    set -l partial_esc (__codon_json_escape "$__codon_partial")
    set -l descr_esc (__codon_json_escape "$__codon_description")
    set -l cwd_esc (__codon_json_escape (pwd))
    set -l payload "{\"id\":1,\"method\":\"agent.complete\",\"params\":{\"partial\":\"$partial_esc\",\"description\":\"$descr_esc\",\"cwd\":\"$cwd_esc\",\"shell\":\"fish\"}}"

    # Visual placeholder while the agent thinks. Replaced before
    # Enter has a chance to fire (Ctrl-G is the trigger, not Enter).
    commandline --replace -- '# … asking codon agent …'

    set -l response (__codon_rpc "$payload")
    set -l rpc_status $status
    if test $rpc_status -ne 0
        commandline --replace -- "$original"
        echo "codon: RPC failed (status $rpc_status)" >&2
        return
    end
    # The response is one JSON line. Success shape:
    #   {"id":1,"ok":{"command_b64":"<base64>"}}
    # Error shape:
    #   {"id":1,"err":{"code":"...","message":"..."}}
    # base64 chars are URL-safe to grep: we don't have to deal with
    # JSON-string escapes at all.
    set -l command_b64 (string match -r '"command_b64"\s*:\s*"([A-Za-z0-9+/=]*)"' -- $response)[2]
    if test -z "$command_b64"
        set -l err_msg (string match -r '"message"\s*:\s*"([^"]*)"' -- $response)[2]
        commandline --replace -- "$original"
        if test -n "$err_msg"
            echo "codon: $err_msg" >&2
        else
            echo "codon: unexpected response: $response" >&2
        end
        return
    end
    set -l command (printf '%s' $command_b64 | base64 -d 2>/dev/null)
    if test -z "$command"
        commandline --replace -- "$original"
        echo "codon: failed to decode response (base64)" >&2
        return
    end
    commandline --replace -- "$command"
    commandline -f end-of-line
end

bind \cg __codon_hash_at_trigger
if bind -M insert >/dev/null 2>&1
    bind -M insert \cg __codon_hash_at_trigger
end
