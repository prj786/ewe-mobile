/**
 * Lucide codepoints, rendered through Icon.svelte and the vendored Lucide font
 * (assets/Lucide.ttf, 1.40.0) so the desktop and its apps stay one icon
 * language. Values are the font's own cmap; the shell's Theme.qml names the
 * same numbers.
 */
export const ICONS = {
  check: 0xE06C,
  caretDown: 0xE06D, // chevron-down
  caretRight: 0xE06F, // chevron-right
  chevronLeft: 0xE06E,
  plus: 0xE13D,
  x: 0xE1B2,
  info: 0xE0F9,
  alert: 0xE077, // circle-alert
  warning: 0xE193, // triangle-alert
  success: 0xE226, // circle-check
  refresh: 0xE145, // refresh-cw
  external: 0xE0B9, // external-link
  copy: 0xE09E,
  pencil: 0xE1F9,
  trash: 0xE18E, // trash-2
  more: 0xE0B6, // ellipsis
  arrowLeft: 0xE048,
  search: 0xE151,
  user: 0xE19F,
  users: 0xE1A4,
  settings: 0xE154,
  // the side navigation
  phone: 0xE163, // smartphone
  messages: 0xE117, // message-square
  contacts: 0xE09C, // contact
  bell: 0xE059,
  music: 0xE122,
  folder: 0xE0D7,
  touchpad: 0xE449,
  devices: 0xE3A2, // monitor-smartphone
  // devices by kind
  tablet: 0xE17E,
  laptop: 0xE1CD,
  monitor: 0xE11D,
  tv: 0xE195,
  // the phone's state
  battery: 0xE053,
  batteryCharging: 0xE054,
  batteryFull: 0xE055,
  batteryMedium: 0xE057,
  batteryLow: 0xE056,
  batteryWarning: 0xE3AC,
  signal: 0xE25F,
  signalHigh: 0xE260,
  signalMedium: 0xE262,
  signalLow: 0xE261,
  signalZero: 0xE263,
  wifiOff: 0xE1AF,
  // actions
  call: 0xE133, // phone (the handset)
  callIn: 0xE136, // phone-incoming
  callMissed: 0xE137, // phone-missed
  messagePlus: 0xE40C, // message-square-plus
  send: 0xE152,
  reply: 0xE22A,
  ring: 0xE224, // bell-ring
  ping: 0xE497, // radar
  clipboard: 0xE225, // clipboard-copy
  upload: 0xE19E,
  download: 0xE0B2,
  folderOpen: 0xE247,
  file: 0xE0C0,
  image: 0xE0F6,
  paperclip: 0xE12D,
  link: 0xE102,
  unlink: 0xE19C,
  shield: 0xE1FF, // shield-check
  keyboard: 0xE284,
  pointer: 0xE1C3, // mouse-pointer-2
  home: 0xE0F5,
  recents: 0xE167, // square
  accessibility: 0xE297,
  // the player
  play: 0xE13C,
  pause: 0xE12E,
  next: 0xE160, // skip-forward
  previous: 0xE15F, // skip-back
  volume: 0xE1AB, // volume-2
  volumeLow: 0xE1AA, // volume-1
  volumeOff: 0xE1AC, // volume-x
  disc: 0xE494, // disc-3
  mail: 0xE10F,
  eject: 0xE10E // log-out: disconnect the phone's storage
};
