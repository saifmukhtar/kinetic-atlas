import socket, ipaddress
from monitor_networks import is_public_ip

for ip_str in ["8.8.8.8", "1.1.1.1", "127.0.0.1", "192.168.1.5", "169.254.169.254", "10.0.0.1"]:
    print(f"{ip_str} -> {is_public_ip(ip_str)}")
