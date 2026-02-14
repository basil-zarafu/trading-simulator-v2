#!/usr/bin/env python3
"""
Web server for Trading Simulator V2 with intraday support
Uses Rust binary with --json flag for data exchange
"""

import http.server
import socketserver
import json
import subprocess
import os
from dataclasses import dataclass
from typing import List, Dict

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
    ohlc: List[Dict]
    price_points: List[Dict]
    config: Dict

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
        """Run simulation using Rust binary with JSON output"""
        days = data.get('days', 30)
        initial_price = data.get('initial_price', 62.0)
        volatility = data.get('volatility', 0.30)
        vrp = data.get('vrp', 0.05)
        seed = data.get('seed', 42)
        strategy = data.get('strategy', 'straddle')
        
        # Build config file
        config_path = self.build_config(days, initial_price, volatility, vrp, seed, strategy)
        
        # Run Rust binary with JSON output
        try:
            result = subprocess.run(
                [SIMULATOR_BIN, config_path, "--json"],
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,  # Ignore warnings
                text=True,
                timeout=30
            )
            
            # Parse JSON output
            rust_output = json.loads(result.stdout)
            return self.transform_output(rust_output, strategy)
                
        except subprocess.TimeoutExpired:
            return self.error_response("Simulation timed out")
        except Exception as e:
            return self.error_response(f"Simulation error: {str(e)}")
    
    def build_config(self, days, initial_price, volatility, vrp, seed, strategy):
        """Build YAML config for Rust simulator"""
        base_config = f"""simulation:
  days: {days}
  initial_price: {initial_price:.2f}
  drift: 0.0
  volatility: {volatility:.2f}
  volatility_risk_premium: {vrp:.2f}
  seed: {seed}
  risk_free_rate: 0.05
  contract_multiplier: 1000
  intraday_resolution_minutes: 10
  calendar_type: "cl_futures"
"""
        
        if strategy == 'long_protection':
            strategy_config = """strategy:
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
"""
        else:
            # Default 1DTE short straddle
            strategy_config = """strategy:
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
"""
        
        config_yaml = base_config + "\n" + strategy_config + """
strike_config:
  tick_size: 0.25
  roll_type: recenter
"""
        # Write as YAML
        yaml_path = "/tmp/sim_config.yaml"
        with open(yaml_path, 'w') as f:
            f.write(config_yaml)
        
        return yaml_path
    
    def transform_output(self, rust_output, strategy):
        """Transform Rust JSON output to web UI format"""
        trades = rust_output.get('trades', [])
        summary = rust_output.get('summary', {})
        daily_ohlc = rust_output.get('daily_ohlc', [])
        price_points = rust_output.get('price_points', [])
        config = rust_output.get('config', {})
        
        # Calculate win rate
        close_trades = [t for t in trades if t.get('trade_type') == 'close']
        wins = sum(1 for t in close_trades if t.get('pnl', 0) > 0)
        win_rate = (wins / len(close_trades) * 100) if close_trades else 0
        
        # Transform trades for UI (preserve all fields needed for charts)
        ui_trades = []
        for trade in trades:
            ui_trades.append({
                'trade_type': trade.get('trade_type', ''),
                'message': trade.get('message', ''),
                'day': trade.get('day', 0),
                'minute': trade.get('minute', 0),
                'price': trade.get('price', 0),
                'pnl': trade.get('pnl')  # May be null for open trades
            })
        
        # Calculate P&L per day
        days = config.get('days', 30)
        net_pnl = summary.get('net_pnl', 0) * 1000  # Convert to dollars
        pnl_per_day = net_pnl / days if days > 0 else 0
        
        return {
            'net_pnl': net_pnl,
            'position_count': summary.get('total_positions', 0),
            'win_rate': win_rate,
            'final_price': summary.get('final_price', 0),
            'trades': ui_trades,
            'ohlc': daily_ohlc,
            'price_points': price_points,
            'config': config,
            # Per-day metrics
            'short_pnl': net_pnl if strategy != 'long_protection' else 0,
            'long_pnl': net_pnl if strategy == 'long_protection' else 0,
            'short_pnl_per_day': pnl_per_day if strategy != 'long_protection' else 0,
            'long_pnl_per_day': pnl_per_day if strategy == 'long_protection' else 0,
            'total_pnl_per_day': pnl_per_day,
            'short_positions': summary.get('total_positions', 0) if strategy != 'long_protection' else 0,
            'long_positions': summary.get('total_positions', 0) if strategy == 'long_protection' else 0,
            'short_win_rate': win_rate if strategy != 'long_protection' else 0,
            'long_win_rate': win_rate if strategy == 'long_protection' else 0,
        }
    
    def error_response(self, message):
        """Return error response"""
        return {
            'error': message,
            'net_pnl': 0,
            'position_count': 0,
            'win_rate': 0,
            'final_price': 0,
            'trades': [{'trade_type': 'hold', 'message': f'Error: {message}'}],
            'ohlc': [],
            'price_points': [],
        }

if __name__ == '__main__':
    print("🚀 Trading Simulator Web Server starting...")
    print(f"📱 Open http://localhost:{PORT} in your browser")
    print("")
    
    socketserver.TCPServer.allow_reuse_address = True
    with socketserver.TCPServer(("127.0.0.1", PORT), SimHandler) as httpd:
        httpd.serve_forever()
