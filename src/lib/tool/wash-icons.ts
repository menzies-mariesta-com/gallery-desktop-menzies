/**
 * Lucide icon nodes for Svelte (Wash UI lucide-react 1.28.0).
 * Sync `__iconNode` only. No top-level await (WebKitGTK TDZ).
 * Note: lucide `filter-x.mjs` only re-exports default; import `funnel-x` for `__iconNode`.
 */
import { __iconNode as arrowUp } from 'lucide-react/dist/esm/icons/arrow-up.mjs';
import { __iconNode as blend } from 'lucide-react/dist/esm/icons/blend.mjs';
import { __iconNode as check } from 'lucide-react/dist/esm/icons/check.mjs';
import { __iconNode as circleCheck } from 'lucide-react/dist/esm/icons/circle-check.mjs';
import { __iconNode as circleX } from 'lucide-react/dist/esm/icons/circle-x.mjs';
import { __iconNode as chevronDown } from 'lucide-react/dist/esm/icons/chevron-down.mjs';
import { __iconNode as chevronLeft } from 'lucide-react/dist/esm/icons/chevron-left.mjs';
import { __iconNode as chevronRight } from 'lucide-react/dist/esm/icons/chevron-right.mjs';
import { __iconNode as clipboardCopy } from 'lucide-react/dist/esm/icons/clipboard-copy.mjs';
import { __iconNode as contrast } from 'lucide-react/dist/esm/icons/contrast.mjs';
import { __iconNode as copy } from 'lucide-react/dist/esm/icons/copy.mjs';
import { __iconNode as crop } from 'lucide-react/dist/esm/icons/crop.mjs';
import { __iconNode as download } from 'lucide-react/dist/esm/icons/download.mjs';
import { __iconNode as droplets } from 'lucide-react/dist/esm/icons/droplets.mjs';
import { __iconNode as filterX } from 'lucide-react/dist/esm/icons/funnel-x.mjs';
import { __iconNode as flame } from 'lucide-react/dist/esm/icons/flame.mjs';
import { __iconNode as flipHorizontal2 } from 'lucide-react/dist/esm/icons/flip-horizontal-2.mjs';
import { __iconNode as flipVertical2 } from 'lucide-react/dist/esm/icons/flip-vertical-2.mjs';
import { __iconNode as folder } from 'lucide-react/dist/esm/icons/folder.mjs';
import { __iconNode as gripVertical } from 'lucide-react/dist/esm/icons/grip-vertical.mjs';
import { __iconNode as image } from 'lucide-react/dist/esm/icons/image.mjs';
import { __iconNode as minus } from 'lucide-react/dist/esm/icons/minus.mjs';
import { __iconNode as moon } from 'lucide-react/dist/esm/icons/moon.mjs';
import { __iconNode as palette } from 'lucide-react/dist/esm/icons/palette.mjs';
import { __iconNode as pencil } from 'lucide-react/dist/esm/icons/pencil.mjs';
import { __iconNode as puzzle } from 'lucide-react/dist/esm/icons/puzzle.mjs';
import { __iconNode as rotateCcw } from 'lucide-react/dist/esm/icons/rotate-ccw.mjs';
import { __iconNode as rotateCw } from 'lucide-react/dist/esm/icons/rotate-cw.mjs';
import { __iconNode as refreshCw } from 'lucide-react/dist/esm/icons/refresh-cw.mjs';
import { __iconNode as scaling } from 'lucide-react/dist/esm/icons/scaling.mjs';
import { __iconNode as search } from 'lucide-react/dist/esm/icons/search.mjs';
import { __iconNode as snowflake } from 'lucide-react/dist/esm/icons/snowflake.mjs';
import { __iconNode as sparkles } from 'lucide-react/dist/esm/icons/sparkles.mjs';
import { __iconNode as square } from 'lucide-react/dist/esm/icons/square.mjs';
import { __iconNode as sun } from 'lucide-react/dist/esm/icons/sun.mjs';
import { __iconNode as trash } from 'lucide-react/dist/esm/icons/trash.mjs';
import { __iconNode as typeIcon } from 'lucide-react/dist/esm/icons/type.mjs';
import { __iconNode as users } from 'lucide-react/dist/esm/icons/users.mjs';
import { __iconNode as x } from 'lucide-react/dist/esm/icons/x.mjs';
import type { WashIconNode } from '$lib/tool/wash-icon-node';

export type { WashIconNode };

export const washIcons = {
	sun,
	download,
	minus,
	square,
	copy,
	x,
	trash,
	check,
	'circle-check': circleCheck,
	'circle-x': circleX,
	'arrow-up': arrowUp,
	'chevron-down': chevronDown,
	'chevron-left': chevronLeft,
	'chevron-right': chevronRight,
	'filter-x': filterX,
	folder,
	'grip-vertical': gripVertical,
	image,
	pencil,
	puzzle,
	'refresh-cw': refreshCw,
	'rotate-cw': rotateCw,
	'rotate-ccw': rotateCcw,
	'flip-horizontal-2': flipHorizontal2,
	'flip-vertical-2': flipVertical2,
	crop,
	scaling,
	contrast,
	blend,
	palette,
	droplets,
	sparkles,
	moon,
	flame,
	snowflake,
	'clipboard-copy': clipboardCopy,
	search,
	type: typeIcon,
	users
} as const satisfies Record<string, WashIconNode>;
