# 软件中心 base.sh 的最小模拟
export KSROOT=/koolshare
export PATH=$KSROOT/bin:$KSROOT/scripts:/usr/sbin:/usr/bin:/sbin:/bin:$PATH
echo_date() { echo "$(date '+%Y-%m-%d %T'): $*"; }
http_response() { echo "$1" >>/tmp/lab/http_response.log; }
base64_encode() { base64 | tr -d '\n'; }
base64_decode() { base64 -d; }
