import os
import sys
import json
import socket
import urllib.request
import urllib.parse
import ipaddress

def is_public_ip(ip_str):
    try:
        ip = ipaddress.ip_address(ip_str)
        return ip.is_global and not (ip.is_private or ip.is_loopback or ip.is_link_local or ip.is_multicast or ip.is_reserved)
    except ValueError:
        return False

def ping_host(host, port, timeout=5):
    """Attempt a TCP connection to the specified host and port."""
    try:
        ip = socket.gethostbyname(host)
        if not is_public_ip(ip):
            print(f"Ping rejected for {host}:{port} - resolves to private/reserved IP {ip}")
            return False
            
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(timeout)
        s.connect((ip, int(port)))
        s.close()
        return True
    except Exception as e:
        print(f"Ping failed for {host}:{port} - {e}")
        return False

def check_url(url):
    try:
        parsed = urllib.parse.urlparse(f"http://{url}" if "://" not in url else url)
        host = parsed.hostname
        if not host: return False
        ip = socket.gethostbyname(host)
        if not is_public_ip(ip):
            print(f"Error: {host} resolves to private/reserved IP {ip}")
            return False
            
        req = urllib.request.Request(f"http://{ip}{parsed.path or '/'}", headers={'User-Agent': 'Mozilla/5.0', 'Host': host})
        urllib.request.urlopen(req, timeout=5)
        return True
    except Exception:
        try:
            req = urllib.request.Request(f"https://{ip}{parsed.path or '/'}", headers={'User-Agent': 'Mozilla/5.0', 'Host': host})
            urllib.request.urlopen(req, timeout=5)
            return True
        except Exception as e2:
            print(f"HTTP/HTTPS Check failed for {url} - {e2}")
            return False

def main():
    if not os.path.exists('networks'):
        print("No networks directory found.")
        return

    # Load previous uptime state
    state_file = '.github/scripts/uptime_state.json'
    uptime_state = {}
    if os.path.exists(state_file):
        try:
            with open(state_file, 'r') as f:
                uptime_state = json.load(f)
        except Exception as e:
            print(f"Failed to load uptime_state.json: {e}")

    # 1. Parse ATLAS.md existing rows
    try:
        with open('ATLAS.md', 'r') as f:
            content = f.read()
    except FileNotFoundError:
        print("ATLAS.md not found.")
        return

    lines = content.split('\n')
    header_lines = []
    table_rows = {}
    table_started = False
    
    for line in lines:
        if line.startswith('| :---'):
            table_started = True
            header_lines.append(line)
            continue
            
        if not table_started:
            header_lines.append(line)
        elif line.startswith('|'):
            parts = [p.strip() for p in line.split('|')[1:-1]]
            if len(parts) >= 8:
                tld = parts[0].replace('`', '')
                table_rows[tld] = parts
        else:
            header_lines.append(line)

    # 2. Iterate through json files and check liveness
    for filename in os.listdir('networks'):
        if not filename.endswith('.json'):
            continue
            
        tld = filename[:-5]
        filepath = os.path.join('networks', filename)
        
        try:
            with open(filepath, 'r') as f:
                config = json.load(f)
                
            seed = config.get('seed_domain')
            nodes = config.get('bootstrap_nodes', [])
            
            if not seed or not nodes:
                print(f"Skipping {tld}: Missing seed or nodes.")
                continue
                
            # Liveness Logic: ONLY Bootstrap Nodes
            is_live = False
            for node in nodes:
                try:
                    parts = node.split('/')
                    if 'ip4' in parts and 'tcp' in parts:
                        ip = parts[parts.index('ip4') + 1]
                        port = parts[parts.index('tcp') + 1]
                        if ping_host(ip, port):
                            is_live = True
                            break
                except Exception:
                    pass
            
            # Apply 7-day rule
            if tld not in uptime_state:
                uptime_state[tld] = {"consecutive_failures": 0}
                
            if is_live:
                uptime_state[tld]["consecutive_failures"] = 0
                status_badge = "🟢 Active"
            else:
                uptime_state[tld]["consecutive_failures"] += 1
                failures = uptime_state[tld]["consecutive_failures"]
                print(f"{tld} is offline. Consecutive failures: {failures}")
                
                # Only mark offline if failures >= 7
                if failures >= 7:
                    status_badge = "🔴 Offline"
                else:
                    # Still show as active to users, or warning
                    status_badge = "🟢 Active"
            
            if tld in table_rows:
                table_rows[tld][3] = status_badge
            else:
                print(f"Warning: {tld} found in networks/ but missing from ATLAS.md")
                
        except Exception as e:
            print(f"Error processing {filename}: {e}")
            
    # 3. Rewrite ATLAS.md
    sorted_tlds = sorted(table_rows.keys())
    
    with open('ATLAS.md', 'w') as f:
        f.write('\n'.join(header_lines[:header_lines.index('| :---') + 1]) + '\n')
        for tld in sorted_tlds:
            row_str = "| " + " | ".join(table_rows[tld]) + " |"
            f.write(row_str + '\n')
            
    # 4. Save state
    os.makedirs(os.path.dirname(state_file), exist_ok=True)
    with open(state_file, 'w') as f:
        json.dump(uptime_state, f, indent=4)
        
    print("Successfully completed daily network monitoring.")

if __name__ == '__main__':
    main()
