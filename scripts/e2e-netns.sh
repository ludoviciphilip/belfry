#!/bin/bash
# End-to-end test of belfryd without sudo: runs inside `unshare -rn`.
# Host daemon (A) in this netns, guest daemon (B) in a child netns, joined by a veth pair.
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
D=$ROOT/target/debug/belfryd
ENG=$ROOT/src-tauri/engine
W=${TMPDIR:-/tmp}/belfry-e2e; rm -rf "$W"; mkdir -p "$W"
ip link set lo up
unshare -n sleep 300 & BPID=$!
sleep 0.3
ip link add vA type veth peer name vB netns $BPID
ip addr add 192.168.77.1/24 dev vA; ip link set vA up
nsenter -t $BPID -n sh -c 'ip link set lo up; ip addr add 192.168.77.2/24 dev vB; ip link set vB up'
inB() { nsenter -t $BPID -n "$@"; }

BELFRY_DATA_DIR=$W/A BELFRY_ADVERTISE_V4=192.168.77.1 BELFRY_ADVERTISE_V6=none \
  $D run --engine-dir "$ENG" > $W/A.out 2>&1 & APID=$!
BELFRY_DATA_DIR=$W/B BELFRY_ADVERTISE_V4=none BELFRY_ADVERTISE_V6=none \
  nsenter -t $BPID -n $D run --engine-dir "$ENG" > $W/B.out 2>&1 & BDPID=$!
# Whatever happens, stop both daemons (they stop their engines) on exit.
trap 'kill $APID $BDPID 2>/dev/null; wait 2>/dev/null; kill $BPID 2>/dev/null' EXIT
sleep 1
api() { # api A|B METHOD PATH [json]
  local who=$1 m=$2 p=$3 body=${4:-}
  local tok; tok=$(cat $W/$who/token)
  local cmd=(curl -s -X "$m" -H "Authorization: Bearer $tok" -H 'Content-Type: application/json' "http://127.0.0.1:47830$p")
  [ -n "$body" ] && cmd+=(-d "$body")
  if [ "$who" = B ]; then inB "${cmd[@]}"; else "${cmd[@]}"; fi
}
j() { python3 -c "import json,sys; d=json.load(sys.stdin); print($1)"; }

echo "== sem token -> 401"; curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:47830/v1/status
echo "== A cria sala"
R1=$(api A POST /v1/rooms '{"name":"Bloodborne coop","nickname":"Luis"}')
ID1=$(echo "$R1" | j "d['id']"); CODE1=$(echo "$R1" | j "d['code']")
echo "id=$ID1 code=${CODE1:0:40}... addrs=$(echo "$R1" | j "d['public_addrs']")"
echo "== B entra com o código"
api B POST /v1/join "{\"code\":\"$CODE1\",\"nickname\":\"Amigo\"}" | j "d['name'], d['role'], d['state']"
echo "== código inválido"; api B POST /v1/join '{"code":"BLFXYZ","nickname":"x"}'; echo
for i in $(seq 1 20); do
  s=$(api B GET /v1/status | j "d['rooms'][0]['state']")
  [ "$s" = online ] && break; sleep 1
done
echo "== status A"; api A GET /v1/status | j "[(r['name'], r['state'], r['my_ip'], [(m['name'], m['ip'], m['link'], m['is_host']) for m in r['members']]) for r in d['rooms']]"
echo "== status B"; api B GET /v1/status | j "[(r['name'], r['state'], r['my_ip'], [(m['name'], m['ip'], m['link'], m['is_host']) for m in r['members']]) for r in d['rooms']]"
HOST1=$(echo "$R1" | j "d['host_ip']")
echo "== jogo: B conecta em $HOST1:9307 (TCP) e UDP"
python3 - "$HOST1" "$BPID" <<'EOF'
import socket, threading, subprocess, sys, time
host, bpid = sys.argv[1], sys.argv[2]
s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1); s.bind((host, 9307)); s.listen()
u = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); u.bind((host, 9307))
def tcp():
    c, a = s.accept(); c.send(b'pong:' + c.recv(64))
def udp():
    d, a = u.recvfrom(64); u.sendto(b'pong:' + d, a)
threading.Thread(target=tcp, daemon=True).start(); threading.Thread(target=udp, daemon=True).start()
client = f'''
import socket
c = socket.create_connection(("{host}", 9307), 5); c.send(b"tcp-hunter"); print("TCP:", c.recv(64))
u = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); u.settimeout(5); u.sendto(b"udp-hunter", ("{host}", 9307)); print("UDP:", u.recvfrom(64)[0])
'''
print(subprocess.run(['nsenter', '-t', bpid, '-n', 'python3', '-c', client], capture_output=True, text=True, timeout=15).stdout.strip())
EOF
echo "== segunda sala ao mesmo tempo"
R2=$(api A POST /v1/rooms '{"name":"Minecraft","nickname":"Luis"}')
CODE2=$(echo "$R2" | j "d['code']"); echo "porta da sala 2: $(echo "$R2" | j "d['port']")"
api B POST /v1/join "{\"code\":\"$CODE2\",\"nickname\":\"Amigo\"}" > /dev/null
sleep 8
api B GET /v1/status | j "[(r['name'], r['state'], r['my_ip']) for r in d['rooms']]"
echo "== interfaces em B"; inB ip -4 -br addr | grep blf
echo "== B desliga a sala 1"
api B POST /v1/rooms/$ID1/stop '{}' | j "d['state']"; sleep 1; inB ip -4 -br addr | grep blf
echo "== B apaga a sala 1"; api B DELETE /v1/rooms/$ID1; echo
echo "== reinicio do daemon A mantém salas"
kill $APID; wait $APID 2>/dev/null; sleep 1
echo "engines restantes em A: $(ps -eo comm,args | grep -c '[e]asytier-core --network-name.*Luis')"
BELFRY_DATA_DIR=$W/A BELFRY_ADVERTISE_V4=192.168.77.1 BELFRY_ADVERTISE_V6=none \
  $D run --engine-dir "$ENG" >> $W/A.out 2>&1 & APID=$!
sleep 9
api A GET /v1/status | j "[(r['name'], r['state'], len(r['members'])) for r in d['rooms']]"
echo "== log do daemon A"; head -3 $W/A.out
echo "== fim"
