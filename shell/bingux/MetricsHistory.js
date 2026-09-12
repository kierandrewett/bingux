// Keep only sampled values: missing readings and gaps are never plotted as zero.
function append(history, sample, at) {
    const extra = sample.extra || {};
    const point = {
        at: at,
        cpu: sample.cpuPercent,
        memory: sample.memoryTotalBytes > 0 ? (sample.memoryUsedBytes / sample.memoryTotalBytes) * 100 : null,
        memoryUsed: sample.memoryUsedBytes,
        memoryTotal: sample.memoryTotalBytes,
        receive: sample.networkReceiveBytesPerSecond,
        send: sample.networkTransmitBytesPerSecond,
        temperature: extra.cpuTemperatureCelsius ?? null,
        load: extra.load1 ?? null,
        swap:
            extra.swapTotalBytes > 0
                ? (extra.swapUsedBytes / extra.swapTotalBytes) * 100
                : extra.swapTotalBytes === 0
                  ? 0
                  : null,
        swapUsed: extra.swapUsedBytes ?? null,
        swapTotal: extra.swapTotalBytes ?? null,
        diskRead: extra.diskReadBytesPerSecond ?? null,
        diskWrite: extra.diskWriteBytesPerSecond ?? null,
    };
    for (const core of extra.cpuCores || []) point["cpu" + core.id] = core.usage;
    return history
        .filter((entry) => entry.at > at - 300000 && entry.at < at)
        .slice(-300)
        .concat([point]);
}

function valid(value) {
    return typeof value === "number" && Number.isFinite(value) && value >= 0;
}

function windowPoints(history, end, duration) {
    return history.filter((point) => point.at >= end - duration && point.at <= end);
}

function peak(points, keys) {
    let result = 0;
    for (const point of points) for (const key of keys) if (valid(point[key])) result = Math.max(result, point[key]);
    return result;
}

function rateMaximum(points, keys) {
    return Math.pow(2, Math.ceil(Math.log2(Math.max(1024, peak(points, keys)))));
}

function nearest(points, at) {
    let found = null;
    for (const point of points) if (!found || Math.abs(point.at - at) < Math.abs(found.at - at)) found = point;
    // Do not pretend a historical point describes a gap or an unsampled future.
    return found && Math.abs(found.at - at) <= 2500 ? found : null;
}

function segments(points, key) {
    const result = [];
    let segment = [];
    for (const point of points) {
        if (!valid(point[key]) || (segment.length && point.at - segment[segment.length - 1].at > 5000)) {
            if (segment.length) result.push(segment);
            segment = [];
        }
        if (valid(point[key])) segment.push(point);
    }
    if (segment.length) result.push(segment);
    return result;
}
