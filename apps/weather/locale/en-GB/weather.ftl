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

## The hero card

weather-hero-here = My location
weather-hero-range = High { $high }, low { $low }
weather-hero-feels = Feels like { $temperature }

## The week and the hourly strip

weather-week-title = Daily forecast
weather-week-wind = wind up to { $speed }
weather-week-uv = UV { $index }
weather-week-low-high = Low { $low }, high { $high }
weather-week-highs-lows = Highs from { $high_low } to { $high_high }, lows from { $low_low } to { $low_high }
weather-hourly-title = Hourly forecast
weather-hourly-now = Now
weather-hourly-sunrise = Sunrise at { $time }
weather-hourly-sunset = Sunset at { $time }
weather-hourly-wind = Wind { $speed }{ $from }
weather-hourly-chance = { $chance } chance of rain
weather-hourly-range = Next 24 hours: low { $low }, high { $high }.
weather-hourly-sunrise-short = Sunrise
weather-hourly-sunset-short = Sunset

## The cards

weather-card-uv = UV index
weather-card-uv-label = UV index { $index }, { $level }
weather-card-wind = Wind
weather-card-wind-with-direction = { $direction } wind, { $speed }
weather-card-wind-speed = Wind, { $speed }
weather-card-gusts = Gusts up to { $speed }
weather-card-sun = Sun
weather-card-sun-range = Sunrise { $rise }, sunset { $set }
weather-card-sunrise-next = Sunrise { $time }
weather-card-sunset-next = Sunset { $time }
weather-card-sunrise-tomorrow = Sunrise { $time } tomorrow
weather-card-no-sun = No sunrise or sunset
weather-card-daylight = Daylight { $length }
weather-card-moon = Moon
weather-card-full-moon = Full moon
weather-card-new-moon = New moon
