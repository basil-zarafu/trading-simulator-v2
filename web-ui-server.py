#!/usr/bin/env python3
"""
Simple HTTP server for Trading Simulator V2
Uses only Python standard library (no Flask required)
"""

import http.server
import socketserver
import json
import subprocess
import os
import re
from urllib.parse import urlparse

PORT = 3000
SIMULATOR_BIN = "/home/gutastef/trading-simulator-v2/target/debug/trading-simulator-v2"
UI_DIR = "/home/gutastef/trading-simulator-v2/ui"

class SimHandler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=UI_DIR, **kwargs)
    
    def do_POST(self):
        if self.path == '/run':
            content_length = int(self.headers['Content-Length'])
            post_data = self.rfile.read(content_length)
            data = json.loads(post_data)
            
            result = self.run_simulation(data)
            
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.end_headers()
            self.wfile.write(json.dumps(result).encode())
        else:
            self.send_response(404)
            self.end_headers()
    
    def run_simulation(self, data):
        days = data.get('days', 30)
        initial_price = data.get('initial_price', 75.0)
        volatility = data.get('volatility', 0.30)
        vrp = data.get('vrp', 0.05)
        seed = data.get('seed', 42)
        strategy = data.get('strategy', 'straddle')
        
        # Generate config based on strategy
        if strategy == 'long_protection':
            config_yaml = f"""simulation:
  days: {days}
  initial_price: {initial_price:.2f}
  drift: 0.0
  volatility: {volatility:.2f}
  volatility_risk_premium: {vrp:.2f}
  seed: {seed}
  risk_free_rate: 0.05
  contract_multiplier: 1000

strategy:
  strategy_type: straddle
  entry_dte: 70
  entry_time: "15:00"
  roll_time: "14:00"
  strike_selection: OTM
  strike_offset: 3.0
  side: "long"
  roll_triggers:
    - trigger_type: dte
      value: 28.0
      legs: both
    - trigger_type: profit_target
      value: 0.14
      legs: both

strike_config:
  tick_size: 0.25
  roll_type: recenter
"""
        else:
            # Default 1DTE short straddle
            config_yaml = f"""simulation:
  days: {days}
  initial_price: {initial_price:.2f}
  drift: 0.0
  volatility: {volatility:.2f}
  volatility_risk_premium: {vrp:.2f}
  seed: {seed}
  risk_free_rate: 0.05
  contract_multiplier: 1000

strategy:
  strategy_type: straddle
  entry_dte: 1
  entry_time: "15:00"
  roll_time: "14:00"
  strike_selection: ATM
  side: "short"
  roll_triggers:
    - trigger_type: time
      value: 14.0
      legs: both

strike_config:
  tick_size: 0.25
  roll_type: recenter
"""
        
        # Write config file
        config_path = "/tmp/sim_config.yaml"
        with open(config_path, 'w') as f:
            f.write(config_yaml)
        
        # Run simulation
        result = subprocess.run(
            [SIMULATOR_BIN, config_path],
            capture_output=True,
            text=True
        )
        
        stdout = result.stdout
        
        # Parse output
        trades = self.parse_trades(stdout)
        net_pnl, position_count, final_price = self.extract_summary(stdout)
        
        # Calculate win rate by parsing P&L from close trades
        # A win is when P&L > 0 (positive return)
        win_rate = 0.0
        if position_count > 0:
            wins = 0
            for t in trades:
                if t['trade_type'] == 'close':
                    # Extract P&L from message like "P&L: $-5197" or "P&L: $1234"
                    match = re.search(r'P&L:\s*\$(-?[0-9,]+)', t['message'])
                    if match:
                        pnl = float(match.group(1).replace(',', ''))
                        if pnl > 0:
                            wins += 1
            win_rate = (wins / position_count) * 100.0
        
        return {
            'net_pnl': net_pnl,
            'position_count': position_count,
            'win_rate': win_rate,
            'final_price': final_price,
            'trades': trades
        }
    
    def parse_trades(self, output):
        trades = []
        for line in output.split('\n'):
            if 'OPENED position' in line:
                trades.append({'trade_type': 'open', 'message': line.strip()})
            elif 'CLOSED position' in line:
                trades.append({'trade_type': 'close', 'message': line.strip()})
            elif 'Holding pos' in line:
                trades.append({'trade_type': 'hold', 'message': line.strip()})
        return trades
    
    def extract_summary(self, output):
        net_pnl = 0.0
        position_count = 0
        final_price = 0.0
        
        for line in output.split('\n'):
            if 'Net P&L:' in line:
                # Match ($-24,604 total) or ($24,604 total) - capture minus sign
                match = re.search(r'\(\$?(-?[0-9,]+) total\)', line)
                if match:
                    net_pnl = float(match.group(1).replace(',', ''))
            
            if 'Total positions opened:' in line:
                parts = line.split(':')
                if len(parts) > 1:
                    try:
                        position_count = int(parts[1].strip())
                    except:
                        pass
            
            if 'Final underlying price:' in line:
                match = re.search(r'\$([0-9.]+)', line)
                if match:
                    final_price = float(match.group(1))
        
        return net_pnl, position_count, final_price

if __name__ == '__main__':
    print("🚀 Trading Simulator Web Server starting...")
    print("📱 Open http://localhost:3000 in your browser")
    print("")
    
    socketserver.TCPServer.allow_reuse_address = True
    with socketserver.TCPServer(("127.0.0.1", PORT), SimHandler) as httpd:
        httpd.serve_forever()
