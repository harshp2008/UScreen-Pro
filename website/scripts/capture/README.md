<!--
  website/scripts/capture/README.md: how the pictures and videos on the landing page were made.
  Owns the capture recipe; the images themselves are in website/public/real and website/public/video.
-->
# Capturing the real app

Every screenshot and recording on the landing page is the real desktop app (`uscreen-pro-gui`) running its
development mode (`npm run dev`, port 1420), which uses made-up sample data: a tablet called SM-X616B, a phone, a serial
and an address that do not exist. No tablet or daemon is involved, and the pictures say "sample data".

The scripts drive a private headless Chrome over the DevTools protocol and send real mouse events to that page only:

```bash
npm --prefix uscreen-pro-gui run dev                      # the app's development mode on :1420
google-chrome --headless=new --remote-debugging-port=9333 --user-data-dir="$(mktemp -d)" about:blank &
node website/scripts/capture/wizard.mjs out/            # the Add device screens (add '?bootstrap=blocked' for the router-blocked state)
node website/scripts/capture/settings.mjs out/          # the settings pages
VW=960 VH=600 node website/scripts/capture/record.mjs wizard out/vid   # a screencast, then encode with ffmpeg
```

`record.mjs` writes the frames and an `ffconcat` list with each frame's real duration; encode it with
`ffmpeg -f concat -safe 0 -i list.txt -vf fps=30,format=yuv420p -c:v libx264 -crf 26 out.mp4`.
The settings regions on the page are crops of the 2x screenshots. Photos of a real tablet are not here: the tablet app's own screens need
a real device and are captured by hand.
