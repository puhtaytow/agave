#!/bin/bash
LOGFILE="nextest_runs_$(date +%Y%m%d_%H%M%S).log"
COMMAND="cargo +nightly-2025-02-16 nextest run --test-threads 1"
cleanup() {
echo "$(date): SIGTERM SAVE..." >> "$LOGFILE"
echo "SIGETERM DATE: $(date)" >> "$LOGFILE"
echo "----------------------------------------" >> "$LOGFILE"
}
trap cleanup SIGTERM SIGINT
while true; do
echo "$(date): RUN..." >> "$LOGFILE"
$COMMAND >> "$LOGFILE" 2>&1
EXIT_CODE=$?
echo "$(date): EXIT CODE: $EXIT_CODE" >> "$LOGFILE"
echo "----------------------------------------" >> "$LOGFILE"
if [ $EXIT_CODE -eq 0 ]; then
echo "OK"
break
fi
echo "RETRY..."
sleep 5
done

niech kazdy retry zapisuje do nowego pliku