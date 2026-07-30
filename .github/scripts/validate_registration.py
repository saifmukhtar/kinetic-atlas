import os
import sys
import re
import socket
import json
import urllib.request

def check_url(url):
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
        ip = socket.gethostbyname(host)
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
            header = line[4:].strip().lower()
            if 'network name' in header: current_key = 'name'
            elif 'tld suffix' in header: current_key = 'tld'
            elif 'description' in header: current_key = 'desc'
            elif 'project website' in header: current_key = 'url'
            elif 'repository url' in header: current_key = 'repo'
            elif 'binary download' in header: current_key = 'binary'
            elif 'logo url' in header: current_key = 'logo_url'
            elif 'seed domain' in header: current_key = 'seed'
            elif 'network id' in header: current_key = 'network_id'
            elif 'local bind ip' in header: current_key = 'local_bind_ip'
            elif 'api port' in header: current_key = 'api_port'
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
        
    lines = content.split('\n')
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
            parts = [p.strip() for p in line.split('|')[1:-1]]
            if len(parts) >= 5:
                row_tld = parts[0].replace('`', '')
                table_rows.append((row_tld, line))
        else:
            header_lines.append(line)
            
    repo_link = f"[{repo}]({repo})" if repo else "N/A"
    url_link = f"[{url}]({url})" if url else "N/A"
    new_row = f"| `{tld}` | {name} | {network_type} | {status} | {desc} | {local_bind_ip} | {url_link} | {repo_link} |"
    table_rows = [r for r in table_rows if r[0] != tld]
    table_rows.append((tld, new_row))
    table_rows.sort(key=lambda x: x[0])
    
    with open('ATLAS.md', 'w') as f:
        f.write('\n'.join(header_lines[:header_lines.index('| :---') + 1]) + '\n')
        for _, row in table_rows:
            f.write(row + '\n')

def main():
    issue_body = os.environ.get('ISSUE_BODY', '')
    issue_user = os.environ.get('ISSUE_USER', '')
    
    print(f"Processing issue from {issue_user}...")
    
    data = parse_issue_body(issue_body)
    
    # Validate required fields
    required = ['version', 'name', 'tld', 'desc', 'url', 'seed', 'network_id', 'local_bind_ip', 'api_port']
    for req in required:
        if req not in data or not data[req]:
            print(f"Error: Missing required field '{req}'")
            sys.exit(1)
            
    tld = data['tld'].lower().strip()
    if not re.match(r'^[a-z0-9]+$', tld):
        print(f"Error: TLD '{tld}' must be alphanumeric.")
        sys.exit(1)

    try:
        api_port = int(data['api_port'].strip())
    except ValueError:
        print(f"Error: API Port '{data['api_port']}' must be a valid integer.")
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
    print(f"Checking liveness of seed domain URL: {data['seed']}...")
    if check_url(data['seed']):
        print("Seed domain is accessible!")
    else:
        print("Error: Could not reach seed domain URL. Please ensure it is publicly accessible.")
        sys.exit(1)
        
    # Generate JSON
    config = {
        "version": data['version'],
        "network_id": data['network_id'],
        "tld": tld,
        "local_bind_ip": data['local_bind_ip'],
        "api_port": api_port,
        "seed_domain": data['seed'],
        "bootstrap_nodes": unique_nodes,
    }
    
    # Check Logo
    if data.get('logo_url'):
        logo_url = data['logo_url'].strip()
        print(f"Validating logo URL: {logo_url}")
        try:
            req = urllib.request.Request(logo_url, method="HEAD", headers={'User-Agent': 'Mozilla/5.0'})
            with urllib.request.urlopen(req, timeout=5) as response:
                content_length = response.headers.get('Content-Length')
                content_type = response.headers.get('Content-Type')
                
                if content_length and int(content_length) > 50000:
                    print(f"Error: Logo size ({content_length} bytes) exceeds 50KB limit.")
                    sys.exit(1)
                    
                if content_type not in ['image/png', 'image/svg+xml']:
                    print(f"Error: Logo must be PNG or SVG. Got {content_type}.")
                    sys.exit(1)
                    
            print("Logo passed HEAD check. Downloading...")
            os.makedirs("logos", exist_ok=True)
            ext = ".png" if content_type == "image/png" else ".svg"
            logo_path = f"logos/{tld}{ext}"
            
            get_req = urllib.request.Request(logo_url, headers={'User-Agent': 'Mozilla/5.0'})
            with urllib.request.urlopen(get_req, timeout=10) as get_response:
                with open(logo_path, "wb") as f:
                    f.write(get_response.read())
                    
            config["logo"] = f"https://raw.githubusercontent.com/saifmukhtar/kinetic-atlas/main/{logo_path}"
        except Exception as e:
            print(f"Error validating or downloading logo: {e}")
            sys.exit(1)
    
    if data.get('ipfs'):
        config["ipfs_gateway"] = data['ipfs']

    if data.get('repo'):
        config["repo"] = data['repo']
        
    if data.get('binary'):
        config["binary_download"] = data['binary']
        
    json_path = f"networks/{tld}.json"
    with open(json_path, 'w') as f:
        json.dump(config, f, indent=4)
        
    # Update ATLAS.md
    update_atlas_md(tld, data['name'], data.get('network_type', 'Public'), data['desc'], data['local_bind_ip'], data['url'], data.get('repo', ''))
    
    # Aggregate all networks into root index.json
    all_networks = []
    if os.path.exists("networks"):
        for filename in sorted(os.listdir("networks")):
            if filename.endswith(".json"):
                with open(os.path.join("networks", filename), 'r') as nf:
                    try:
                        all_networks.append(json.load(nf))
                    except Exception as e:
                        print(f"Error loading {filename}: {e}")
                        
    with open("index.json", "w") as idx_file:
        json.dump(all_networks, idx_file, indent=4)
        
    print("Successfully processed registration and updated index.json.")

if __name__ == '__main__':
    main()
