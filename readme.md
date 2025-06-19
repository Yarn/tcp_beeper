# TCP Beeper

Beeps each type a byte is received from a TCP server.

`--volume` can be used to adjust the volume, 1 is the default, 0.5 is half volume, etc

`--min-rate` can be used to not beep until bytes are received at a certain rate, averages over a 4 second window  
for example `--min-rate 2` will only beep if bytes are received at at least 2hz

## Example

`nc -l -p 9000 -e "bash -c 'while true; do echo y;sleep 0.25; done'"`

`cargo run -- '127.0.0.1:9000'`
