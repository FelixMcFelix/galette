ADDR1=$( cat /sys/class/net/ve1/address )
ADDR2=$( cat /sys/class/net/ve2/address )

sudo ip addr add 192.170.20.01 dev ve1
sudo ip addr add 192.170.20.02 dev ve2

sudo arp -s 192.170.20.01 $ADDR1 -i ve2
sudo arp -s 192.170.20.02 $ADDR2 -i ve1

sudo ip link set dev ve1 up
sudo ip link set dev ve2 up
