#!/usr/bin/env fish

# Drive the Phase 24 reference FM navigation workload on macOS.
# Start Codon with:
#   codon --render-trace=/tmp/codon-fm.jsonl
# Focus a file-manager pane containing at least 500 entries, then run
# this script from another terminal.

if not command -q cliclick
    echo "render-trace-replay: install cliclick (brew install cliclick)" >&2
    exit 1
end

set duration_seconds 60
if test (count $argv) -ge 1
    set duration_seconds $argv[1]
end

set started (date +%s)
while test (math (date +%s) - $started) -lt $duration_seconds
    cliclick kp:j
    sleep 0.06
    cliclick kp:k
    sleep 0.06
    cliclick kp:h
    sleep 0.20
    cliclick kp:l
    sleep 0.20
end
