function choose(windows, desktopId, startupClass, title, allowFallback) {
    const normalize = (value) =>
        String(value || "")
            .replace(/\.desktop$/, "")
            .toLowerCase();
    const identities = [normalize(desktopId), normalize(startupClass)].filter(Boolean);
    const matches = windows.filter((window) => identities.includes(normalize(window.appId)));
    const query = String(title || "").toLowerCase();
    const searched = query
        ? matches.find((window) =>
              String(window.title || "")
                  .toLowerCase()
                  .includes(query),
          )
        : null;
    return searched || (allowFallback ? matches.find((window) => window.activated) || matches[0] : null) || null;
}
