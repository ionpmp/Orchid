# Jyotish widget

The Jyotish widget is a local, offline Vedic panchanga (tithi / nakshatra /
yoga / karana / vara) with an optional personal layer, day-quality "traffic
light" scores, and a birth-time rectification wizard. Everything is computed
on-device from the current position and clock — there is no network call and
no ephemeris file.

This document covers what the score means, the two data layers it can draw
on, the inauspicious daylight windows, how to set up birth data, the
rectification wizard, and the accuracy limits you should keep in mind before
treating any of this as authoritative.

## What the score means

The day-quality score is a **heuristic traffic light**, not a substitute for
a full muhurta consultation. It starts from a base value and adds/subtracts
signed points for classical panchanga factors — tithi quality (Rikta /
Purna-class / Nanda-Bhadra-Jaya flow), Amavasya/Purnima, yoga tension
(Vyatipata/Vaidhriti and other tense yogas), and karana (Vishti, fixed
karanas). When birth data is present it additionally layers tara (nakshatra
counting from the natal moon) and a chandrashtama-style check of which house
the transiting Moon occupies from the natal Moon.

The resulting score is mapped to three colors:

- **Green** — favorable (score ≥ 65)
- **Yellow** — mixed (40–64)
- **Red** — challenging (< 40)

Two related scores are shown side by side on the Day tab: the **now** score
(sampled at the current instant, only meaningful for today) and the **day**
score (sampled at local solar noon, representing the day as a whole). Under
the headline, a short Fluent **summary** (`jyotish-summary-{color}-{mode}-{0|1}`)
explains the day in plain language; influence and advice lines then spell out
what it means and what to do. The factor breakdown lists the top contributions
ranked by strength so the number is never a black box.

## Personal vs. panchanga layers

- **Panchanga layer** (always available): tithi, nakshatra, yoga, karana,
  vara, and the corresponding day-quality rules — no birth data needed.
- **Personal layer** (unlocked once a birth date is set): adds tara analysis
  and the transiting-Moon house check against your natal Moon, tunes the
  narrative/advice text to your chart, and powers the Life tab's
  year-by-year retrospective and current Vimshottari daśā (mahā / antar /
  pratyantar) display.

The Day tab shows a small badge indicating which layer is currently active
("Panchanga" or "Personal") so it's always clear whether a given score used
your birth data.

Widget settings can also toggle **Use personal (natal) day layer**. When off,
birth data stays stored but scores, Life/daśā, and gochara notes fall back to
the generic panchanga layer — useful for comparing “with / without natal”.

## Rahu, Yama, and Gulika (inauspicious windows)

Below sunrise/sunset the widget shows three daylight windows classically
avoided for new beginnings, each an eighth of the sunrise–sunset span
assigned by weekday:

- **Rahu Kalam** — the best-known of the three; the Day tab highlights it in
  red and can send a notification the moment it begins (see below).
- **Yamagandam**
- **Gulika Kalam** (Gulika/Mandi)

All three require sunrise/sunset to be computable for the configured
location, which in turn requires a location with a resolvable latitude and
longitude (the default is Varanasi). **Show Rahu Kalam / Yamagandam / Gulika**
in settings hides the Day-tab ranges without disabling Rahu-Kalam
notifications.

### Multi-location picker

The location list always starts with a pinned **Current location** row. When
selected (`use_current` in config), sunrise / muhurta / panchanga timing use
coordinates resolved for this session: Windows Location API first, then an
IP geolocation fallback if GPS is denied or fails. Resolved coords are cached
on the live widget handle; selecting Current again refreshes them. Busy and
error states use the Fluent keys `jyotish-current-locating` /
`jyotish-current-failed`.

Tap the location name on the Day tab to open the location picker. From there
you can search for a place by name (Open-Meteo geocoding, same provider as
the Weather widget), add it to your saved locations, switch the active
location via the chip strip or the picker list, and remove any location you
no longer need (at least one location is always kept). Locations are no
longer edited from the settings panel — latitude/longitude/name fields there
have been replaced by this in-widget picker.

## Engineering notes

- Day colors for Month/Year grids are memoized per civil date (`color_cache`);
  changing active-profile birth data, ayanamsa, the personal-layer toggle, or
  the active observation location (saved city, Current resolve, or add/remove)
  invalidates it, since sunrise and muhurta windows are location-dependent.
- Live APIs: `select_current_location`, `refresh_current_location`, profile
  picker helpers (see above), plus the existing city picker helpers.
- `JyotishPayload` is `Box`’d in snapshots; the closed rectify wizard skips
  quiz/chrome Fluent strings on every UI patch.
- Legacy flat `birth_*` config fields migrate into `profiles[0]` on decode;
  birth place defaults to the first observation location.

## Birth profiles

Birth data lives in **profiles** (people), not in the widget settings panel.
Tap **Add profile** on the Day chip strip (or the Life-tab prompt) to open
the profile picker — the same overlay pattern as the location list.

Each profile stores:

- **Name** and optional **gender** (UI / future narrative only; score math
  ignores gender)
- **Birth date** (`YYYY-MM-DD`)
- **Birth time** (`HH:MM`, local clock time at birth)
- **Birth UTC offset (minutes)** — the UTC offset that applied at the birth
  location/date (e.g. `330` for IST), since the widget does not carry a
  timezone database
- **Birth place** (name + lat/lon) — searched via Open-Meteo geocoding,
  **separate** from observation locations

Observation location (including Current) still drives sunrise / muhurta /
Rahu Kalam only. Natal Moon, tara, daśā, and the rectification wizard use
the **active profile** (birth datetime + birth place for lagna).

Once the active profile has a birth date, `has_birth_data` unlocks the
personal layer, the Life tab's retrospective, and the current daśā display.
Birth time matters for the ascendant/houses used by rectification; if you
don't know it precisely, use the rectification wizard below rather than
guessing.

Widget settings keep ayanamsa, show toggles, notifications, and **Use
personal (natal) day layer** — not the birth fields themselves.

Live APIs: `set_profile_picker_open`, `select_profile`, `remove_profile`,
`begin_edit_profile`, `upsert_profile`, `search_birth_places`,
`set_draft_birth_place`, `nav_profile_cal`, `set_profile_cal_mode`,
`set_profile_cal_day`, `set_profile_time` (plus gender draft helper).

## Rectification overview

If your birth time is uncertain, the built-in wizard narrows it down without
requiring an outside tool:

1. **Uncertainty window** — pick roughly how wrong the known time might be
   (±30 min, ±2 h, ±6 h, or "unknown time of day" for a full-day scan).
2. **Quiz** — answer a short set of questions about temperament and life
   pattern; each answer scores candidate ascendants.
3. **Life events** — optionally add dated life events (marriage, career
   change, relocation, etc.); each event scores candidates against
   Vimshottari daśā/antardaśā transitions.
4. **Results** — ranked candidate windows with a quiz/event/total score
   breakdown; you can **refine** (narrow further around the top result) or
   **accept** the top candidate as your rectified birth time.

Accepting a candidate only updates the active profile's birth time — it never
touches birth date, birth place, observation location, or ayanamsa. The
wizard keeps a resumable draft if you close it partway through. An open draft
resyncs to the active profile's **birth place** when the profile or place
changes.

## Accuracy limits

- Planetary and lunar positions use simplified formulas from Jean Meeus'
  *Astronomical Algorithms*, not a Swiss Ephemeris–grade integration. This is
  accurate enough for panchanga/traffic-light purposes over ordinary human
  timescales but will drift arc-minutes to degrees from a professional
  ephemeris at the edges of its valid range (this project targets
  roughly 1900–2100).
- The ayanamsa (Lahiri / Krishnamurti / Raman) shifts sidereal longitudes by
  a fixed offset model with linear precession — again a good approximation,
  not a research-grade constant.
- Sunrise/sunset (and therefore Rahu Kalam/Yamagandam/Gulika and the vara
  boundary) use a standard geometric formula and do not account for
  atmospheric refraction edge cases at extreme latitudes.
- Birth-time rectification is a **statistical aid**, not a certified
  solution — it ranks candidates by how well they match your quiz answers
  and life events; it does not replace consultation with a professional
  astrologer.
- The day-quality score is one heuristic model among many valid schools of
  thought in Jyotish. Treat green/yellow/red as orientation, not
  prescription — the disclaimer shown on the Day tab is not boilerplate.

## Notifications

Two independent toggles live in the widget's settings panel:

- **Notify when today's day score color changes** — fires once per color
  change of *today's* day score (green/yellow/red), with a body matching the
  new color.
- **Notify when Rahu Kalam begins** — fires once on the rising edge into
  today's Rahu Kalam window.

Both default to **on**. Notifications only fire for the "today" view of an
instance and never on the very first observation of a newly opened/restored
widget (so reopening the app doesn't immediately spam a notification from
whatever state the day happens to be in).

## Export to clipboard

The Day tab has **Copy day** / **Copy week** buttons (shown once loaded):

- **Copy day** builds a plain-text summary of the selected day: date,
  location, score color and value, headline, tithi/nakshatra/yoga/karana/
  vara, Rahu Kalam/Yamagandam/Gulika windows when available, and the advice
  lines.
- **Copy week** builds a compact weekday → color summary for the 7-day strip
  around the selected day.

Both copy to the system clipboard and confirm with a notification.

## Universal search

Typing keywords like `jyotish`, `panchanga`, `rahu`, `rahukalam`, `tithi`,
`nakshatra`, `yoga`, `karana`, `muhurta`, or `dasha` into universal search
surfaces a Jyotish hit for each live Jyotish widget (also matched against
the widget's configured location name). Activating a `today`/`rahu`-style
hit jumps that widget to today; other keyword hits keep the widget's
currently viewed day.
