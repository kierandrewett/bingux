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
# $chance: the chance of rain as a percentage.
weather-week-chance = { $chance } chance of rain
# Separator between the parts of a day or hour's detail line.
weather-list-separator = ,{" "}
weather-week-low-high = Low { $low }, high { $high }
weather-week-highs-lows = Highs from { $high_low } to { $high_high }, lows from { $low_low } to { $low_high }
weather-hourly-title = Hourly forecast
weather-hourly-now = Now
weather-hourly-sunrise = Sunrise at { $time }
weather-hourly-sunset = Sunset at { $time }
# $speed: the wind speed with its unit.
weather-hourly-wind = Wind { $speed }
# $speed: the wind speed with its unit. $point: the compass point, short.
weather-hourly-wind-from = Wind { $speed } from { $point }
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


## UV scale, one per band; inline is the lowercase form for a sentence; advice is one line
weather-uv-low = Low
weather-uv-inline-low = low
weather-uv-advice-low = No protection needed.
weather-uv-moderate = Moderate
weather-uv-inline-moderate = moderate
weather-uv-advice-moderate = Wear sunscreen if you are outside for long.
weather-uv-high = High
weather-uv-inline-high = high
weather-uv-advice-high = Wear sunscreen and a hat. Seek shade at midday.
weather-uv-very-high = Very high
weather-uv-inline-very-high = very high
weather-uv-advice-very-high = Cover up and wear sunscreen. Avoid the midday sun.
weather-uv-extreme = Extreme
weather-uv-inline-extreme = extreme
weather-uv-advice-extreme = Stay out of the sun at midday.

## Air quality, one per US AQI band; inline is lowercase; advice is one line
weather-aqi-good = Good
weather-aqi-inline-good = good
weather-aqi-advice-good = Air quality is satisfactory.
weather-aqi-moderate = Moderate
weather-aqi-inline-moderate = moderate
weather-aqi-advice-moderate = Unusually sensitive people may notice effects.
weather-aqi-unhealthy-sensitive = Unhealthy for sensitive groups
weather-aqi-inline-unhealthy-sensitive = unhealthy for sensitive groups
weather-aqi-advice-unhealthy-sensitive = People with lung or heart disease should limit time outside.
weather-aqi-unhealthy = Unhealthy
weather-aqi-inline-unhealthy = unhealthy
weather-aqi-advice-unhealthy = Everyone may notice effects. Limit time outside.
weather-aqi-very-unhealthy = Very unhealthy
weather-aqi-inline-very-unhealthy = very unhealthy
weather-aqi-advice-very-unhealthy = Avoid exertion outside.
weather-aqi-hazardous = Hazardous
weather-aqi-inline-hazardous = hazardous
weather-aqi-advice-hazardous = Stay indoors.

## Dew point comfort, one per band
weather-dew-dry = Dry
weather-dew-comfortable = Comfortable
weather-dew-humid = Humid
weather-dew-very-humid = Very humid
weather-dew-oppressive = Oppressive

## Visibility, one per band
weather-visibility-excellent = Excellent
weather-visibility-clear = Clear
weather-visibility-moderate = Moderate
weather-visibility-poor = Poor
weather-visibility-very-poor = Very poor

## Pressure trend
weather-pressure-rising = Rising
weather-pressure-falling = Falling
weather-pressure-steady = Steady

## Wind force, Beaufort plain names
weather-force-calm = Calm
weather-force-light-breeze = Light breeze
weather-force-breeze = Breeze
weather-force-strong-breeze = Strong breeze
weather-force-gale = Gale
weather-force-storm = Storm
weather-force-hurricane-force = Hurricane force

## Compass points: short code, long word, and the title form that starts a sentence
# Short code for the hourly forecast's wind
weather-point-short-n = N
weather-point-short-nne = NNE
weather-point-short-ne = NE
weather-point-short-ene = ENE
weather-point-short-e = E
weather-point-short-ese = ESE
weather-point-short-se = SE
weather-point-short-sse = SSE
weather-point-short-s = S
weather-point-short-ssw = SSW
weather-point-short-sw = SW
weather-point-short-wsw = WSW
weather-point-short-w = W
weather-point-short-wnw = WNW
weather-point-short-nw = NW
weather-point-short-nnw = NNW
# Long word, lower case, for a sentence
weather-point-long-n = north
weather-point-long-nne = north-north-east
weather-point-long-ne = north-east
weather-point-long-ene = east-north-east
weather-point-long-e = east
weather-point-long-ese = east-south-east
weather-point-long-se = south-east
weather-point-long-sse = south-south-east
weather-point-long-s = south
weather-point-long-ssw = south-south-west
weather-point-long-sw = south-west
weather-point-long-wsw = west-south-west
weather-point-long-w = west
weather-point-long-wnw = west-north-west
weather-point-long-nw = north-west
weather-point-long-nnw = north-north-west
# Title form, for a card label
weather-point-title-n = North
weather-point-title-nne = North-north-east
weather-point-title-ne = North-east
weather-point-title-ene = East-north-east
weather-point-title-e = East
weather-point-title-ese = East-south-east
weather-point-title-se = South-east
weather-point-title-sse = South-south-east
weather-point-title-s = South
weather-point-title-ssw = South-south-west
weather-point-title-sw = South-west
weather-point-title-wsw = West-south-west
weather-point-title-w = West
weather-point-title-wnw = West-north-west
weather-point-title-nw = North-west
weather-point-title-nnw = North-north-west
## Card sentence: the dew point
# $temperature: the dew point, already formatted with its unit.
weather-card-dew-point = The dew point is { $temperature } right now.

## Card label for the air quality range
# $index: the AQI number. $level: the band in lower case, from aqi_inline.
weather-card-aqi-label = Air quality index { $index }, { $level }

## Notice headings: one per kind of notice
weather-notice-thunder-hail-title = Thunderstorms with hail
weather-notice-thunder-title = Thunderstorms
weather-notice-rain-title = Heavy rain
weather-notice-ice-title = Freezing rain
weather-notice-snow-title = Heavy snow
weather-notice-wind-strong-title = Strong wind
weather-notice-wind-gusty-title = Gusty wind
weather-notice-heat-title = Extreme heat
weather-notice-cold-title = Extreme cold
weather-notice-uv-title = Very high UV
weather-notice-air-unhealthy-title = Unhealthy air
weather-notice-air-poor-title = Poor air for sensitive groups

## Notice bodies
# $amount: the rain, already formatted with its unit. $when: the time span from weather-when-*.
weather-notice-rain-detail = Up to { $amount } an hour. { $when }
# $amount: the rain, already formatted with its unit.
weather-notice-rain-detail-untimed = Up to { $amount } an hour.
# $when: the time span from weather-when-*.
weather-notice-ice-detail = Roads and paths may be icy. { $when }
# $speed: the gust, already formatted with its unit. $when: the time span from weather-when-*.
weather-notice-wind-detail = Gusts up to { $speed }. { $when }
# $speed: the gust, already formatted with its unit.
weather-notice-wind-detail-untimed = Gusts up to { $speed }.
# $temperature: the apparent temperature, already formatted with its unit.
weather-notice-heat-detail = It feels like { $temperature }. Drink water and avoid the afternoon sun.
# $temperature: the apparent temperature, already formatted with its unit.
weather-notice-cold-detail = It feels like { $temperature }. Cover exposed skin.
# $when: the time span from weather-when-*.
weather-notice-uv-detail = UV index 8 or more. { $when }
# $index: the air quality index, rounded.
weather-notice-air-detail = Air quality index { $index }.

## Time spans: a stretch of the day, as a phrase
# $time: a clock time.
weather-when-until = Until { $time }.
# $time: a clock time, on the next day.
weather-when-until-tomorrow = Until { $time } tomorrow.
# $from: a clock time. $to: a clock time.
weather-when-between = From { $from } to { $to }.
# $from: a clock time, on the next day. $to: a clock time.
weather-when-from-tomorrow-to = From { $from } tomorrow to { $to }.
# $from: a clock time. $to: a clock time, on the next day.
weather-when-between-tomorrow = From { $from } to { $to } tomorrow.

## Calendar words: weekdays, months, the day-of-month and the clock period
# Weekday names, Monday first.
weather-weekday-monday = Monday
weather-weekday-tuesday = Tuesday
weather-weekday-wednesday = Wednesday
weather-weekday-thursday = Thursday
weather-weekday-friday = Friday
weather-weekday-saturday = Saturday
weather-weekday-sunday = Sunday
# Weekday names shortened to three letters, Monday first.
weather-weekday-short-monday = Mon
weather-weekday-short-tuesday = Tue
weather-weekday-short-wednesday = Wed
weather-weekday-short-thursday = Thu
weather-weekday-short-friday = Fri
weather-weekday-short-saturday = Sat
weather-weekday-short-sunday = Sun
# Month names, January first.
weather-month-january = January
weather-month-february = February
weather-month-march = March
weather-month-april = April
weather-month-may = May
weather-month-june = June
weather-month-july = July
weather-month-august = August
weather-month-september = September
weather-month-october = October
weather-month-november = November
weather-month-december = December
# $day: the day of the month. $month: the month name.
weather-day-month = { $day } { $month }
# $percent: the moon's lit share, a whole number. $phase: the next phase, such as Full moon. $date: a day and month.
weather-moon-note = { $percent }% lit. { $phase } on { $date }.
# $hour: the hour, 1 to 12. $period: AM or PM.
weather-clock-hour-12h = { $hour } { $period }
# $hour: the hour, 1 to 12. $minute: two digits. $period: AM or PM.
weather-clock-time-12h = { $hour }:{ $minute } { $period }
weather-period-am = AM
weather-period-pm = PM
