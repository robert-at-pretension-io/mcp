#!/bin/bash
set -e

FILE="src/computer_actions.rs"
TEMPFILE=$(mktemp)

echo "Fixing syntax in $FILE..."

# Fix the handle_initialize function return
sed -i '786s/    \/\/ Return the result and next action/    \/\/ Return the session info with the response/' $FILE

# Check if the handle_execute_action function is complete
if ! grep -q "Ok(serde_json::to_string_pretty(&result)?)" $FILE; then
  echo "Function handle_execute_action appears incomplete or has syntax errors"
fi

# Ensure all functions have proper return types
grep -n "fn " $FILE | while read -r line; do
  line_num=$(echo "$line" | cut -d: -f1)
  if ! echo "$line" | grep -q "-> Result<"; then
    echo "Line $line_num: Function may be missing proper return type"
  fi
done

# Check for unpaired braces
opening_braces=$(grep -o "{" $FILE | wc -l)
closing_braces=$(grep -o "}" $FILE | wc -l)
if [ "$opening_braces" != "$closing_braces" ]; then
  echo "Unpaired braces detected: $opening_braces opening vs $closing_braces closing"
fi

echo "Basic syntax check complete"
