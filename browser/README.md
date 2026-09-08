# YouTube artwork for MPRIS

`youtube-mpris-artwork.user.js` fills missing Media Session artwork on YouTube,
including VORAPIS. Helium then sends that thumbnail through its normal MPRIS
interface. The script preserves artwork already supplied by the page and uses
the playing video's ID rather than a title search. It does not change playback.

Install it with an existing userscript manager such as Tampermonkey. Open the
`.user.js` file's URL and select **Install**. It runs only on `www.youtube.com`.
Disable or remove “Bingux YouTube MPRIS artwork” in the manager to remove it.

The initial live check applied the script to the already-playing page without
reloading it. Installing the script makes it run on subsequent page loads.
The helper checks local metadata every two seconds because VORAPIS can replace
it without a navigation event. YouTube's image server receives a request when
Helium needs to load a new thumbnail.

Run `node --test tests/youtube-mpris-artwork.test.mjs` from the repository root.
