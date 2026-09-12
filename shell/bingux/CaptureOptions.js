// Validate the entire request before changing persistent capture preferences.
function validate(options, bounds) {
    if (!options || typeof options !== "object" || Array.isArray(options))
        throw new Error("Capture options must be an object");
    const choices = {
        kind: ["screenshot", "recording"],
        target: ["region", "screen", "window"],
        format: ["png", "jpeg"],
        quality: ["compact", "balanced", "high"],
        audio: ["none", "system", "microphone", "both"],
        backend: ["auto", "portal"],
        encoder: ["auto", "cpu"],
        fps: [15, 30, 60],
        maxHeight: [0, 720, 1080, 1440, 2160],
        delay: [0, 1, 2, 3, 5, 10],
    };
    for (const key of Object.keys(options)) {
        const value = options[key];
        if (Object.prototype.hasOwnProperty.call(choices, key)) {
            if (!choices[key].includes(value)) throw new Error("Invalid " + key);
        } else if (key === "cursor" || key === "copy") {
            if (typeof value !== "boolean") throw new Error(key + " must be true or false");
        } else if (key === "directory") {
            if (typeof value !== "string" || (value !== "" && !value.startsWith("/")) || value.includes("\u0000"))
                throw new Error("Output directory must be an absolute path");
        } else if (key === "region") {
            if (
                !value ||
                typeof value !== "object" ||
                Object.keys(value).sort().join(",") !== "height,width,x,y" ||
                ![value.x, value.y, value.width, value.height].every(Number.isInteger) ||
                value.x < 0 ||
                value.y < 0 ||
                value.width < 2 ||
                value.height < 2 ||
                !bounds ||
                value.x + value.width > bounds.width ||
                value.y + value.height > bounds.height
            )
                throw new Error("Region must fit inside the selected screen and be at least 2 by 2 pixels");
        } else throw new Error("Unknown capture option: " + key);
    }
    return options;
}
