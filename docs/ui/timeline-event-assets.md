# Chronobreak timeline event assets

These assets are vendored locally for the replay timeline.

## Source

CommunityDragon exports Riot Games client/game assets.

CommunityDragon states that the project operates under Riot Games'
"Legal Jibber Jabber" policy. The underlying game/client artwork is
owned by Riot Games.

Chronobreak does not depend on CommunityDragon at runtime. The selected
files are copied into the application and served locally.

## Presentation semantics

The upstream tower and inhibitor files are Riot team-100/team-200
artwork:

- team 100 uses the blue artwork;
- team 200 uses the red artwork.

Chronobreak therefore vendors those files as `*-blue.png` and
`*-red.png`.

The current `ViewerEvent` contract exposes `ally`, `enemy`, or
`neutral`; it does not expose authoritative Riot team 100/200 identity.
For the current timeline presentation Chronobreak maps:

- ally relation -> blue artwork;
- enemy relation -> red artwork;
- neutral or mixed relation -> neutral fallback marker.

This is a UI presentation convention, not a claim about the event's
actual map-side team ID.

## Files

| Local file | Upstream source |
| --- | --- |
| `dragon.png` | https://raw.communitydragon.org/latest/game/assets/ux/scoreboard/_dragon.png |
| `dragon-air.png` | https://raw.communitydragon.org/latest/game/assets/ux/scoreboard/_clouddrake.png |
| `dragon-earth.png` | https://raw.communitydragon.org/latest/game/assets/ux/scoreboard/_mountaindrake.png |
| `dragon-fire.png` | https://raw.communitydragon.org/latest/game/assets/ux/scoreboard/_infernaldrake.png |
| `dragon-water.png` | https://raw.communitydragon.org/latest/game/assets/ux/scoreboard/_oceandrake.png |
| `dragon-hextech.png` | https://raw.communitydragon.org/latest/game/assets/ux/scoreboard/_hextechdrake.png |
| `dragon-chemtech.png` | https://raw.communitydragon.org/latest/game/assets/ux/scoreboard/_chemtechdrake.png |
| `dragon-elder.png` | https://raw.communitydragon.org/latest/game/assets/ux/scoreboard/_elderdrake.png |
| `baron.png` | https://raw.communitydragon.org/latest/game/assets/ux/scoreboard/_baronnashor.png |
| `herald.png` | https://raw.communitydragon.org/latest/game/assets/ux/scoreboard/_riftherald.png |
| `tower-blue.png` | https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-match-history/global/default/tower-100.png |
| `tower-red.png` | https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-match-history/global/default/tower-200.png |
| `inhibitor-blue.png` | https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-match-history/global/default/inhibitor-100.png |
| `inhibitor-red.png` | https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-match-history/global/default/inhibitor-200.png |

## Deferred

Void Grubs remain on the generic timeline fallback until a suitably
small dedicated Riot asset is selected. The current match-history
`right_icons_grub.png` asset is intentionally not used in this pass.
