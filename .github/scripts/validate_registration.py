import os
import sys
import re
import socket
import json
import json
import urllib.request

def check_url(url):
    import urllib.request
    try:
        req = urllib.request.Request(f"http://{url}", headers={'User-Agent': 'Mozilla/5.0'})
        urllib.request.urlopen(req, timeout=5)
        return True
    except Exception as e:
        try:
            req = urllib.request.Request(f"https://{url}", headers={'User-Agent': 'Mozilla/5.0'})
            urllib.request.urlopen(req, timeout=5)
            return True
        except Exception as e2:
            print(f"HTTP/HTTPS Check failed for {url} - {e2}")
            return False

def ping_host(host, port, timeout=3):
    """Attempt a TCP connection to the specified host and port."""
    try:
        # Resolve hostname to IP first
        ip = socket.gethostbyname(host)
        # Attempt connection
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(timeout)
        s.connect((ip, int(port)))
        s.close()
        return True
    except Exception as e:
        print(f"Ping failed for {host}:{port} - {e}")
        return False

def parse_issue_body(body):
    """Extremely simple parser for the GitHub Issue Form format."""
    data = {}
    lines = body.split('\n')
    current_key = None
    current_val = []
    
    for line in lines:
        if line.startswith('### '):
            if current_key:
                data[current_key] = '\n'.join(current_val).strip()
            # Map headers to keys
            header = line[4:].strip().lower()
            if 'network name' in header: current_key = 'name'
            elif 'tld suffix' in header: current_key = 'tld'
            elif 'description' in header: current_key = 'desc'
            elif 'project website' in header: current_key = 'url'
            elif 'repository url' in header: current_key = 'repo'
            elif 'seed domain' in header: current_key = 'seed'
            elif 'network id' in header: current_key = 'network_id'
            elif 'local bind ip' in header: current_key = 'local_bind_ip'
            elif 'bootstrap nodes' in header: current_key = 'nodes'
            elif 'ipfs gateway' in header: current_key = 'ipfs'
            else: current_key = None
            current_val = []
        elif current_key and line != '_No response_':
            current_val.append(line)
            
    if current_key:
        data[current_key] = '\n'.join(current_val).strip()
        
    return data

def update_atlas_md(tld, name, network_type, desc, local_bind_ip, url, repo, status='🟢 Active'):
    with open('ATLAS.md', 'r') as f:
        content = f.read()
        
    # Find the table rows
    lines = content.split('\n')
    new_lines = []
    table_started = False
    table_rows = []
    header_lines = []
    
    for line in lines:
        if line.startswith('| :---'):
            table_started = True
            header_lines.append(line)
            continue
            
        if not table_started:
            header_lines.append(line)
        elif line.startswith('|'):
            # Parse existing row
            parts = [p.strip() for p in line.split('|')[1:-1]]
            if len(parts) >= 5:
                # Store tuple (tld, line) for sorting
                row_tld = parts[0].replace('`', '')
                table_rows.append((row_tld, line))
        else:
            header_lines.append(line)
            
    # Add new row
    repo_link = f"[{repo}]({repo})" if repo else "N/A"
    url_link = f"[{url}]({url})" if url else "N/A"
    new_row = f"| `{tld}` | {name} | {network_type} | {status} | {desc} | {local_bind_ip} | {url_link} | {repo_link} |"
    # Remove existing row with same TLD if it exists
    table_rows = [r for r in table_rows if r[0] != tld]
    table_rows.append((tld, new_row))
    
    # Sort alphabetically by TLD
    table_rows.sort(key=lambda x: x[0])
    
    # Reconstruct
    with open('ATLAS.md', 'w') as f:
        f.write('\n'.join(header_lines[:header_lines.index('| :---') + 1]) + '\n')
        for _, row in table_rows:
            f.write(row + '\n')

def main():
    issue_body = os.environ.get('ISSUE_BODY', '')
    issue_user = os.environ.get('ISSUE_USER', '')
    labels = os.environ.get('ISSUE_LABELS', '')
    
    print(f"Processing issue from {issue_user}...")
    
    data = parse_issue_body(issue_body)
    
    # Validate required fields
    required = ['name', 'tld', 'desc', 'url', 'seed', 'network_id', 'local_bind_ip']
    for req in required:
        if req not in data or not data[req]:
            print(f"Error: Missing required field '{req}'")
            sys.exit(1)
            
    tld = data['tld'].lower().strip()
    if not re.match(r'^[a-z0-9]+$', tld):
        print(f"Error: TLD '{tld}' must be alphanumeric.")
        sys.exit(1)
        
    nodes_str = data.get('nodes', '')
    nodes = []
    for node in nodes_str.split('\n'):
        node = node.strip()
        if node:
            nodes.append(node)
            
    if not nodes:
        print("Error: Missing bootstrap nodes.")
        sys.exit(1)
        
    unique_nodes = list(set(nodes))
    if len(unique_nodes) != 3:
        print(f"Error: You must provide exactly 3 unique hardcoded bootstrap nodes. Found {len(unique_nodes)} unique node(s).")
        sys.exit(1)
        
    if not all('/p2p/' in node or '/ipfs/' in node for node in nodes):
        print("Error: Bootstrap nodes must be in libp2p multiaddr format containing a PeerID (e.g. /p2p/Qm...)")
        sys.exit(1)
        
    # Check liveness: ONLY Seed Domain URL
    is_live = False
    print(f"Checking liveness of seed domain URL: {data['seed']}...")
    if check_url(data['seed']):
        print("Seed domain is accessible!")
        is_live = True
    else:
        print("Error: Could not reach seed domain URL. Please ensure it is publicly accessible.")
        sys.exit(1)
        
    # Generate JSON
    config = {
        "network_id": data['network_id'],
        "tld": tld,
        "local_bind_ip": data['local_bind_ip'],
        "seed_domain": data['seed'],
        "bootstrap_nodes": unique_nodes,
    }
    
    if data.get('ipfs'):
        config["ipfs_gateway"] = data['ipfs']
        
    json_path = f"networks/{tld}.json"
    with open(json_path, 'w') as f:
        json.dump(config, f, indent=4)
        
    # Update ATLAS.md
    update_atlas_md(tld, data['name'], data.get('network_type', 'Public'), data['desc'], data['local_bind_ip'], data['url'], data.get('repo', ''))
    print("Successfully processed registration.")

if __name__ == '__main__':
    main()
