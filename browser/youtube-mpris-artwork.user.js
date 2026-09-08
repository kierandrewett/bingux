// ==UserScript==
// @name         Bingux YouTube MPRIS artwork
// @namespace    bingux
// @version      1.0.0
// @description  Supply the playing video's thumbnail when YouTube omits Media Session artwork.
// @match        https://www.youtube.com/*
// @grant        none
// @run-at       document-idle
// ==/UserScript==

(() => {
    'use strict';
    let suppliedSource = '';

    function updateArtwork() {
        const metadata = navigator.mediaSession?.metadata;
        const player = document.getElementById('movie_player');
        const videoId = player?.getVideoData?.()?.video_id;
        if (!metadata || !/^[a-zA-Z0-9_-]{11}$/.test(videoId || '')) return;
        const artwork = metadata.artwork;
        // Keep artwork supplied by YouTube or another script. Only replace
        // our own thumbnail when the actual playing video changes.
        if (artwork.length && !artwork.every(image => image.src === suppliedSource)) return;
        const source = `https://i.ytimg.com/vi/${videoId}/hqdefault.jpg`;
        if (artwork.length && suppliedSource === source) return;
        metadata.artwork = [{src: source, sizes: '480x360', type: 'image/jpeg'}];
        suppliedSource = source;
    }

    // VORAPIS and YouTube can replace MediaMetadata without a navigation.
    // This only reads local page state; it does not poll a remote service.
    setInterval(updateArtwork, 2000);
    document.addEventListener('playing', updateArtwork, true);
    document.addEventListener('yt-navigate-finish', updateArtwork);
    updateArtwork();
})();
