#!/usr/bin/env python3
import re

# Sample close trades from long protection
close_trades = [
    "Day 17 (Thu W2): Price $60.43 | CLOSED position 1 at 14:00 | P&L: $-6131 (ProfitTarget)",
    "Day 36 (Tue W5): Price $55.44 | CLOSED position 2 at 14:00 | P&L: $-3854 (ProfitTarget)",
    "Day 56 (Mon W8): Price $56.23 | CLOSED position 3 at 14:00 | P&L: $-5197 (ProfitTarget)"
]

# Calculate P&L from close trades (new correct way)
net_pnl = 0.0
for trade in close_trades:
    match = re.search(r'P&L:\s*\$(-?[0-9,]+)', trade)
    if match:
        net_pnl += float(match.group(1).replace(',', ''))

print(f"Old buggy P&L:  $-24,604 (from summary line)")
print(f"New correct P&L: ${net_pnl:,.0f} (sum of close trades)")
print(f"Expected:        $-15,182 ($-6131 + $-3854 + $-5197)")
print()
print("The fix calculates P&L by summing individual close trade P&Ls")
print("instead of using the buggy summary line.")
