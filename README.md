# enterance 

Forked to work with TERA-Europe classic

Minimal TERA Launcher replacement for [teralib](https://github.com/TheNak976/tera-rust-launcher/)-style launchers.

## enterance and it's contributors are not affiliated with, or sponsored by, or authorized by, KRAFTON or Bluehole.


## use

extract to tera dir

Run enterance once to generate blank ini file

Fill in the missing fields. You can find the values by running `strings Tera-Europe\ Launcher.exe | grep _URL ` on linux

(update is HASH_FILE_URL, world is SERVER_LIST_URL, others are obvious)

### linux

download both windows and linux binaries. extract to tera dir.

update and login with the native binary first, it's easier. just run ./enterance and enter login

in whatever launcher you use, run enterance.exe with args --no-update to launch tera

you might need to run the native version from time to time to get a new auth key I guess