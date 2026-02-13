#!/usr/bin/env python3
"""
Simple HTTP server for Trading Simulator V2
Uses only Python standard library (no Flask required)
Supports combined short + long position tracking with separate P&L metrics
"""

import http.server
import socketserver
import json
import subprocess
import os
import re
from urllib.parse import urlparse
from dataclasses import dataclass
from typing import List, Dict, Tuple

PORT = 3000
SIMULATOR_BIN = "/home/gutastef/trading-simulator-v2/target/debug/trading-simulator-v2"
UI_DIR = "/home/gutastef/trading-simulator-v2/ui"

@dataclass
class SimResult:
    """Results from a single simulation run"""
    net_pnl: float
    position_count: int
    win_rate: float
    final_price: float
    trades: List[Dict]
    days: int

@dataclass
class CombinedMetrics:
    """Combined metrics for short + long positions"""
    short_pnl: float
    long_pnl: float
    total_pnl: float
    short_pnl_per_day: float
    long_pnl_per_day: float
    total_pnl_per_day: float
    short_positions: int
    long_positions: int
    short_win_rate: float
    long_win_rate: float

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
        days = data.get('days', 365)
        initial_price = data.get('initial_price', 62.0)
        volatility = data.get('volatility', 0.30)
        vrp = data.get('vrp', 0.05)
        seed = data.get('seed', 42)
        strategy = data.get('strategy', 'straddle')
        
        if strategy == 'combined':
            # Run both short and long simulations
            short_result = self.run_single_strategy(
                days, initial_price, volatility, vrp, seed, 'short'
            )
            long_result = self.run_single_strategy(
                days, initial_price, volatility, vrp, seed, 'long'
            )
            
            # Calculate combined metrics
            metrics = self.calculate_combined_metrics(short_result, long_result)
            
            # Combine trades for display
            all_trades = self.merge_trades(short_result.trades, long_result.trades)
            
            return {
                'net_pnl': metrics.total_pnl,
                'position_count': metrics.short_positions + metrics.long_positions,
                'win_rate': (metrics.short_win_rate + metrics.long_win_rate) / 2,
                'final_price': short_result.final_price,
                'trades': all_trades,
                # New combined metrics
                'short_pnl': metrics.short_pnl,
                'long_pnl': metrics.long_pnl,
                'short_pnl_per_day': metrics.short_pnl_per_day,
                'long_pnl_per_day': metrics.long_pnl_per_day,
                'total_pnl_per_day': metrics.total_pnl_per_day,
                'short_positions': metrics.short_positions,
                'long_positions': metrics.long_positions,
                'short_win_rate': metrics.short_win_rate,
                'long_win_rate': metrics.long_win_rate,
            }
        else:
            # Single strategy (backward compatible)
            result = self.run_single_strategy(
                days, initial_price, volatility, vrp, seed, strategy
            )
            
            pnl_per_day = result.net_pnl / result.days if result.days > 0 else 0
            
            return {
                'net_pnl': result.net_pnl,
                'position_count': result.position_count,
                'win_rate': result.win_rate,
                'final_price': result.final_price,
                'trades': result.trades,
                'short_pnl': result.net_pnl if strategy != 'long_protection' else 0,
                'long_pnl': result.net_pnl if strategy == 'long_protection' else 0,
                'short_pnl_per_day': pnl_per_day if strategy != 'long_protection' else 0,
                'long_pnl_per_day': pnl_per_day if strategy == 'long_protection' else 0,
                'total_pnl_per_day': pnl_per_day,
                'short_positions': result.position_count if strategy != 'long_protection' else 0,
                'long_positions': result.position_count if strategy == 'long_protection' else 0,
                'short_win_rate': result.win_rate if strategy != 'long_protection' else 0,
                'long_win_rate': result.win_rate if strategy == 'long_protection' else 0,
            }
    
    def run_single_strategy(self, days, initial_price, volatility, vrp, seed, strategy):
        """Run a single strategy simulation"""
        
        if strategy == 'long' or strategy == 'long_protection':
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
        config_path = f"/tmp/sim_config_{strategy}.yaml"
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
        trades = self.parse_trades(stdout, strategy)
        net_pnl, position_count, final_price = self.extract_summary(stdout, trades, strategy)
        win_rate = self.calculate_win_rate(trades)
        
        return SimResult(
            net_pnl=net_pnl,
            position_count=position_count,
            win_rate=win_rate,
            final_price=final_price,
            trades=trades,
            days=days
        )
    
    def calculate_combined_metrics(self, short: SimResult, long: SimResult) -> CombinedMetrics:
        """Calculate combined metrics from short and long results"""
        
        total_pnl = short.net_pnl + long.net_pnl
        days = short.days
        
        return CombinedMetrics(
            short_pnl=short.net_pnl,
            long_pnl=long.net_pnl,
            total_pnl=total_pnl,
            short_pnl_per_day=short.net_pnl / days if days > 0 else 0,
            long_pnl_per_day=long.net_pnl / days if days > 0 else 0,
            total_pnl_per_day=total_pnl / days if days > 0 else 0,
            short_positions=short.position_count,
            long_positions=long.position_count,
            short_win_rate=short.win_rate,
            long_win_rate=long.win_rate,
        )
    
    def merge_trades(self, short_trades: List[Dict], long_trades: List[Dict]) -> List[Dict]:
        """Merge trades from short and long strategies, sorted by day"""
        
        # Add prefix to distinguish trade sources
        for t in short_trades:
            t['source'] = 'SHORT'
        for t in long_trades:
            t['source'] = 'LONG'
        
        # Combine and sort by extracting day number from message
        all_trades = short_trades + long_trades
        
        def extract_day(trade):
            match = re.search(r'Day\s+(\d+)', trade['message'])
            return int(match.group(1)) if match else 0
        
        all_trades.sort(key=extract_day)
        return all_trades
    
    def parse_trades(self, output, source=''):
        """Parse trades from simulation output and fix display for longs"""
        trades = []
        prefix = f"[{source}] " if source else ""
        is_long = 'long' in source.lower()
        
        for line in output.split('\n'):
            if 'OPENED position' in line or '-> OPENED position' in line:
                # Fix premium display for longs - show negative
                msg = line.strip()
                if is_long:
                    # Change $6.13 to $-6.13 for longs (money spent, not received)
                    # Match the premium amount after the pipe and before "per barrel"
                    msg = re.sub(r'\| \$(\d+\.\d{2}) per barrel', r'| $-\1 per barrel', msg)
                    msg = re.sub(r'\(\$(\d+) total\)', r'($-\1 total)', msg)
                trades.append({
                    'trade_type': 'open',
                    'message': prefix + msg
                })
            elif 'CLOSED position' in line:
                trades.append({
                    'trade_type': 'close', 
                    'message': prefix + line.strip()
                })
            elif 'Holding pos' in line:
                trades.append({
                    'trade_type': 'hold',
                    'message': prefix + line.strip()
                })
        return trades
    
    def calculate_win_rate(self, trades):
        """Calculate win rate from close trades"""
        close_trades = [t for t in trades if t['trade_type'] == 'close']
        if not close_trades:
            return 0.0
        
        wins = 0
        for t in close_trades:
            match = re.search(r'P&L:\s*\$(-?[0-9,]+)', t['message'])
            if match:
                pnl = float(match.group(1).replace(',', ''))
                if pnl > 0:
                    wins += 1
        
        return (wins / len(close_trades)) * 100.0
    
    def extract_summary(self, output, trades=None, strategy=''):
        """Extract summary statistics from simulation output"""
        # Calculate P&L from close trades - this is the correct realized P&L
        net_pnl = 0.0
        if trades:
            for t in trades:
                if t['trade_type'] == 'close':
                    match = re.search(r'P&L:\s*\$(-?[0-9,]+)', t['message'])
                    if match:
                        net_pnl += float(match.group(1).replace(',', ''))
        
        # Fallback to summary line if no trades
        if net_pnl == 0.0:
            for line in output.split('\n'):
                if 'Net P&L:' in line:
                    match = re.search(r'\(\$?(-?[0-9,]+) total\)', line)
                    if match:
                        net_pnl = float(match.group(1).replace(',', ''))
        
        position_count = 0
        final_price = 0.0
        
        for line in output.split('\n'):
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
