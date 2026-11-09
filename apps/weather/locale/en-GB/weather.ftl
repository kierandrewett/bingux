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

## Locations, shell, page, and formatting

weather-source-portal = From the location service
weather-source-network = Approximate, from the network address
weather-my-location = My location
weather-remove = Remove
weather-why-denied = Location is turned off for this app
weather-no-connection = No connection
weather-why-unknown = Could not find your location
weather-locating = Finding your location
weather-try-again = Try again
weather-saved-places = Saved places
weather-add-location = Add location
weather-toast-copied = Conditions copied
weather-toast-copy-failed = Could not copy the conditions
weather-command-refresh = Refresh
weather-command-add-location = Add location…
weather-command-copy = Copy conditions
weather-command-next-place = Next place
weather-command-previous-place = Previous place
weather-command-hours = Look at each hour
weather-command-days = Look at each day
weather-command-remove-place = Remove the place in the list
weather-unit-celsius = Celsius
weather-unit-fahrenheit = Fahrenheit
weather-unit-kmh = Kilometres per hour
weather-unit-mph = Miles per hour
weather-unit-ms = Metres per second
weather-unit-mm = Millimetres
weather-unit-inches = Inches
weather-units = Units
weather-automatic = Automatic
weather-use-my-location = Use my location
weather-use-network-location = Use network location
weather-tooltip-add-location = Add location (Ctrl+N)
weather-tooltip-refresh = Refresh (Ctrl+R)
weather-places = Places
weather-ask-location-title = Use your approximate location?
weather-ask-location-allow = Use approximate location
weather-ask-location-search = Search by city
weather-add-city = Add a city
weather-location-unavailable = Your location is not available. Search for a city to see its forecast.
weather-finding-location = Finding your location…
weather-loading = Loading the forecast…
weather-offline-description = Weather needs the internet. Check the connection, then try again.
weather-service-title = Forecast not available
weather-service-description = The forecast service did not answer. Try again in a moment.
weather-hourly-forecast = Hourly forecast
weather-updated = Updated
weather-place = Place
weather-position = Position
weather-data-credit = Weather data by Open-Meteo.com
weather-data-licence = Licensed CC BY 4.0
weather-duration-minutes = { $minutes } min
weather-duration-hours = { $hours } h
weather-duration-hours-minutes = { $hours } h { $minutes } min
weather-word-snow = Snow
weather-word-rain = Rain
weather-outlook-stops = { $word } stops in { $duration }
weather-outlook-for-next = { $word } for the next { $duration }
weather-outlook-starts = { $word } starts in { $duration }
weather-today = Today
weather-tomorrow = Tomorrow

## Sky conditions, one per WMO code

weather-sky-clear = Clear
weather-sky-mostly-clear = Mostly clear
weather-sky-partly-cloudy = Partly cloudy
weather-sky-overcast = Overcast
weather-sky-fog = Fog
weather-sky-freezing-fog = Freezing fog
weather-sky-light-drizzle = Light drizzle
weather-sky-drizzle = Drizzle
weather-sky-heavy-drizzle = Heavy drizzle
weather-sky-light-freezing-drizzle = Light freezing drizzle
weather-sky-freezing-drizzle = Freezing drizzle
weather-sky-light-rain = Light rain
weather-sky-rain = Rain
weather-sky-heavy-rain = Heavy rain
weather-sky-light-freezing-rain = Light freezing rain
weather-sky-freezing-rain = Freezing rain
weather-sky-light-snow = Light snow
weather-sky-snow = Snow
weather-sky-heavy-snow = Heavy snow
weather-sky-snow-grains = Snow grains
weather-sky-light-showers = Light showers
weather-sky-showers = Showers
weather-sky-heavy-showers = Heavy showers
weather-sky-light-snow-showers = Light snow showers
weather-sky-snow-showers = Snow showers
weather-sky-thunderstorm = Thunderstorm
weather-sky-thunderstorm-hail = Thunderstorm with hail
weather-sky-severe-thunderstorm-hail = Severe thunderstorm with hail
weather-sky-cloudy = Cloudy

## Moon phases

weather-moon-new = New moon
weather-moon-waxing-crescent = Waxing crescent
weather-moon-first-quarter = First quarter
weather-moon-waxing-gibbous = Waxing gibbous
weather-moon-full = Full moon
weather-moon-waning-gibbous = Waning gibbous
weather-moon-last-quarter = Last quarter
weather-moon-waning-crescent = Waning crescent

## The conditions as a line of text, to copy or to read aloud.
## Each sentence shape is one message so translators see the whole sentence.
## Sky words in sentences: lowercase forms of the sky labels above.
# Inline sky forms, used inside the conditions sentences
weather-sky-inline-clear = clear
weather-sky-inline-mostly-clear = mostly clear
weather-sky-inline-partly-cloudy = partly cloudy
weather-sky-inline-overcast = overcast
weather-sky-inline-fog = fog
weather-sky-inline-freezing-fog = freezing fog
weather-sky-inline-light-drizzle = light drizzle
weather-sky-inline-drizzle = drizzle
weather-sky-inline-heavy-drizzle = heavy drizzle
weather-sky-inline-light-freezing-drizzle = light freezing drizzle
weather-sky-inline-freezing-drizzle = freezing drizzle
weather-sky-inline-light-rain = light rain
weather-sky-inline-rain = rain
weather-sky-inline-heavy-rain = heavy rain
weather-sky-inline-light-freezing-rain = light freezing rain
weather-sky-inline-freezing-rain = freezing rain
weather-sky-inline-light-snow = light snow
weather-sky-inline-snow = snow
weather-sky-inline-heavy-snow = heavy snow
weather-sky-inline-snow-grains = snow grains
weather-sky-inline-light-showers = light showers
weather-sky-inline-showers = showers
weather-sky-inline-heavy-showers = heavy showers
weather-sky-inline-light-snow-showers = light snow showers
weather-sky-inline-snow-showers = snow showers
weather-sky-inline-thunderstorm = thunderstorm
weather-sky-inline-thunderstorm-hail = thunderstorm with hail
weather-sky-inline-severe-thunderstorm-hail = severe thunderstorm with hail
weather-sky-inline-cloudy = cloudy

# $place: the place name. $temperature: the temperature with its unit. $sky: a weather-sky-inline word.
weather-conditions-both = { $place }: { $temperature }, { $sky }.
# $place: the place name. $temperature: the temperature with its unit.
weather-conditions-temperature = { $place }: { $temperature }.
# $place: the place name. $sky: a weather-sky-inline word.
weather-conditions-sky = { $place }, { $sky }.
# $place: the place name.
weather-conditions-place = { $place }.

# $feels, $high, $low: temperatures with units. Feels-like, day high and day low.
weather-feel-all = feels like { $feels }, high { $high }, low { $low }.
weather-feel-feels-high = feels like { $feels }, high { $high }.
weather-feel-feels-low = feels like { $feels }, low { $low }.
weather-feel-high-low = high { $high }, low { $low }.
weather-feel-feels = feels like { $feels }.
weather-feel-high = high { $high }.
weather-feel-low = low { $low }.

# $speed: wind speed with unit. $direction: a cardinal word, e.g. north-east. $percent: relative humidity.
weather-air-wind-from-humidity = wind { $speed } from the { $direction }, humidity { $percent }%.
weather-air-wind-from = wind { $speed } from the { $direction }.
weather-air-wind-humidity = wind { $speed }, humidity { $percent }%.
weather-air-wind = wind { $speed }.
weather-air-humidity = humidity { $percent }%.
