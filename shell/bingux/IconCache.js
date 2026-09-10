// Active images stay resident. Only unused entries compete for this budget.
function create(maxBytes = 4 * 1024 * 1024, maxEntries = 256) {
    return {references: new Map(), unused: new Map(), sizes: new Map(), maxBytes, maxEntries};
}

function retain(cache, source) {
    if (!source) return;
    cache.references.set(source, (cache.references.get(source) || 0) + 1);
    cache.unused.delete(source);
}

function release(cache, source) {
    if (!source || !cache.references.has(source)) return;
    const count = cache.references.get(source) - 1;
    if (count) cache.references.set(source, count);
    else {
        cache.references.delete(source);
        cache.unused.set(source, true);
    }
}

function touch(cache, source, bytes) {
    if (bytes !== undefined) cache.sizes.set(source, bytes);
    if (!cache.references.has(source)) {
        cache.unused.delete(source);
        cache.unused.set(source, true);
    }
}

function trim(cache) {
    let bytes = 0;
    for (const source of cache.unused.keys()) bytes += cache.sizes.get(source) || 0;
    const removed = [];
    for (const source of cache.unused.keys()) {
        if (bytes <= cache.maxBytes && cache.unused.size <= cache.maxEntries) break;
        bytes -= cache.sizes.get(source) || 0;
        cache.unused.delete(source);
        cache.sizes.delete(source);
        removed.push(source);
    }
    return removed;
}

function stats(cache) {
    let bytes = 0;
    for (const source of cache.unused.keys()) bytes += cache.sizes.get(source) || 0;
    return {activeEntries: cache.references.size, unusedEntries: cache.unused.size, unusedStringBytes: bytes};
}
