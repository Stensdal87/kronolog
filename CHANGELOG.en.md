# Changes

What is new in each version of KronoLog. The list can also be seen in the app
under Settings → "What's new?".

## 1.3.11 (2026-11-27)

- The week view can show the times as decimals (e.g. 1.75) instead of h:mm: click "h:mm" in the top left corner to switch
- Right-clicking a time or a total in the week view copies it as a decimal, ready to paste into Salesforce, also when the times are shown as h:mm
- A small notice by the mouse shows that the number was copied, also when clicking the decimal in Summary

## 1.3.10 (2026-11-26)

- When KronoLog recognizes the person from an earlier Teams call, the project from last time is now filled in right away under "Record the call on", and the call is registered there when it ends. You can change it during the call
- Meetings without a name ("Microsoft Teams meeting") are no longer remembered as a contact, so they are not all suggested on the same project
- Choosing the type Break when adding or editing an entry now picks Internal time as the project automatically

## 1.3.9 (2026-11-26)

- Splitting a call with ⇄ now registers the first part on the call's customer/project, and the rest continues as a call on the chosen project. Before, the whole call was moved and the phone was switched off
- When splitting time (⇄ or ✂ Split), colors and names under the slider show where each part goes: green stays where the time came from, blue moves to the new project
- The call log now records the Teams windows during the whole call, to find out how going straight from a call into a meeting can be detected

## 1.3.8 (2026-11-24)

- When you type a new customer in the call window, you can pick one of the projects from the template for new customers (e.g. Hotline) with one click

## 1.3.7 (2026-11-24)

- The window can now be dragged between monitors with different scaling (e.g. 100% and 125%) without resisting and growing with every attempt

## 1.3.6 (2026-11-23)

- The week grid now has a Link column with 🔗/📁, as in the Summary
- Click a project in the week grid to unfold the week's entries with times and notes; click an entry to edit it
- Right-click a project in the week grid to open its link or show its entries

## 1.3.5 (2026-11-20)

- New "Hours" column in the Summary history: the rounded time as a decimal number, e.g. 1.75 for 1:45. Click the number to copy it (#42)

## 1.3.4 (2026-11-19)

- Long links, e.g. copied from Edge, now also open on Windows (#41)
- For each link you can choose whether it opens in the default browser, Microsoft Edge or Google Chrome (#41)
- A link can also be a folder, e.g. C:\Customers\Company, which opens in File Explorer (#41)

## 1.3.3 (2026-11-19)

- KronoLog remembers which project a call with a contact was registered on and suggests it next time, so you only have to press Enter (#39)
- Customers and projects can have a link, e.g. to Salesforce, which opens from the right-click menu and from the new Link column in History → Summary (#41). The links are included when you share the customer and project list
- It is easier to see what can be clicked in the history: the rounded time, the link, ⚠ and the week's times get a blue background and a hand cursor when the mouse is over them
- The name from Teams is now also found when Teams is signed in with several accounts, and for meetings the meeting title is used (#39)

## 1.3.2 (2026-11-17)

- "Forgot to start the timer?" after sleep now suggests starting from when the computer woke up instead of from when the timer was stopped before sleep (#40)

## 1.3.1 (2026-11-16)

- The name from Teams now goes in the note, also when the call window was already open while it was ringing (#39). The log says so if no name was found

## 1.3.0 (2026-11-13)

- Experimental: when the phone is taken automatically on Windows, the name from the Teams call window is written in the note. The window titles are also written to the call log. Can be turned off under ☰ → Calls
- When several calls are waiting, you can check them and register them all on the same project at once. Each call keeps its own note

## 1.2.0 (2026-11-12)

- "What's new?" in the settings lists the changes in each version, and the window opens by itself the first time after an update

## 1.1.0 (2026-11-12)

- Search the project list when choosing a project for an entry; the list gets shorter as you type
- "+ New customer…" in the entry editor creates the customer right away, with the projects from the template
- Write a note when a call ends
- The day panel can show the project's total time or this week's
- The customer list no longer fills the whole window when there are only a few customers

## 1.0.0 (2026-11-11)

- Time Snapper is now called KronoLog. Your data, autostart and shortcuts move over by themselves
- Daily backup of your data, the last 14 days, which can be restored from the settings
- Week view under History as a timesheet; click a cell to mark the time as billed
- Gaps in the day are shown and can be filled in with one click
- "Forgot to start the timer?": a reminder when you work without tracking time, and the timer can be started back in time
- Share the customer and project list with colleagues as a CSV file
- ⭐ Favorites at the top of the list and in the project picker
- Daily goal per weekday as a green ring in the clock
- Hour budget per project with a warning when 90% is used
