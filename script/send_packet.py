import socket

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

frame = bytes.fromhex(
    "ffffffffffff"  # dst MAC (broadcast)
    "525252523434"  # src MAC (your NIC)
    "0800"          # ethertype (IP, arbitrary)
) + b"hello worldhello worldhello worldhello worldhello worldhello worldhello worldhello worldhello worldhello worldhello worldhello worldhello worldhello worldhello worldhello world"

sock.sendto(frame, ("127.0.0.1", 1235))
