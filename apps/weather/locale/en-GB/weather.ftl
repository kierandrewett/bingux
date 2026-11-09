### Bingux Weather.
### Source language: British English. Conventions: docs/bingux-ui.md,
### "Localisation". Every ID starts with `weather-`.

# The app's name, in the window title and the About window.
weather-title = Weather
# One line in the About window.
weather-description = Forecasts for where you are and the cities you add.

## Banners on the forecast page

# The title of the banner when the forecast is the last one saved. $ago is the age of the saved forecast, such as "5 min ago".
weather-stale-title = Updated { $ago }
weather-stale-offline = Showing the last forecast. Connect to the internet to update it.
weather-stale-service = Showing the last forecast. The service did not answer.
weather-stale-retry = Try again

## Adding a location

weather-add-title = Add location
weather-add-close = Close
weather-add-placeholder = Search for a city
weather-add-hint-title = Search for a city
weather-add-hint-body = Type the name of a city or town.
# $query is the text the person typed.
weather-add-no-results-title = No results for “{ $query }”
weather-add-no-results-body = Check the spelling, or try a larger city nearby.
weather-add-offline-title = No connection
weather-add-offline-body = Searching needs the internet. Check the connection, then try again.
weather-add-failed-title = Search not available
weather-add-failed-body = The search service did not answer. Try again in a moment.
weather-add-searching = Searching…
