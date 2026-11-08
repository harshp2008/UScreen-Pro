#!/usr/bin/env bash
# Redact personal LAN IPs / serials / BSSID from all tracked text files in cwd.
git ls-files -z | xargs -0 grep -IlE 'TABLET_SERIAL|PHONE_SERIAL|192\.168\.1\.(32|77|74|83)|10\.238\.4\.217|[cC]0:[dD]7:[aA][aA]:[dD]4:[fF]2:73' 2>/dev/null | tr '\n' '\0' | xargs -0 -r sed -i -E \
 's/192\.168\.1\.32/192.168.1.100/g;s/192\.168\.1\.77/192.168.1.101/g;s/192\.168\.1\.74/192.168.1.102/g;s/192\.168\.1\.83/192.168.1.103/g;s/10\.238\.4\.217/10.238.4.100/g;s/[cC]0:[dD]7:[aA][aA]:[dD]4:[fF]2:73/00:11:22:33:44:55/g;s/TABLET_SERIAL/TABLET_SERIAL/g;s/PHONE_SERIAL/PHONE_SERIAL/g'
