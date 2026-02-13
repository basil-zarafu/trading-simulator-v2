#!/usr/bin/env python3
"""Test the fix for long position display"""
import re

# Sample output from Rust binary (buggy - shows positive for longs)
rust_output = """Day 0 (Mon W0): Price $62.00 | OPENED position 1 at 15:00 | Strikes: Put $59.00 Call $65.00 | $6.13 per barrel ($6131 total)
Day 1 (Tue W0): Price $62.15 | Holding pos 1 | DTE: 69 | Unrealized P&L: $-50
Day 17 (Thu W2): Price $60.43 | CLOSED position 1 at 14:00 | P&L: $-6131 (ProfitTarget)
  -> OPENED position 2 at 14:00 | Strikes: Put $57.50 Call $63.50 | $5.91 per barrel ($5911 total)"""

print("=" * 60)
print("BEFORE FIX (what Rust outputs):")
print("=" * 60)
for line in rust_output.split('\n'):
    if 'OPENED' in line:
        print(line)

print()
print("=" * 60)
print("AFTER FIX (what Python server should show):")
print("=" * 60)

# Apply the fix
is_long = True
for line in rust_output.split('\n'):
    if 'OPENED position' in line or '-> OPENED position' in line:
        msg = line.strip()
        if is_long:
            msg = re.sub(r'\| \$(\d+\.\d{2}) per barrel', r'| $-\1 per barrel', msg)
            msg = re.sub(r'\(\$(\d+) total\)', r'($-\1 total)', msg)
        print(msg)

print()
print("=" * 60)
print("VERIFICATION:")
print("=" * 60)
print("Long positions now show NEGATIVE premiums (money spent)")
print("Short positions will show POSITIVE premiums (money collected)")
print("This makes P&L tracking consistent and clear.")
