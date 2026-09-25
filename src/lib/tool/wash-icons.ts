/**
 * Lucide icon nodes for Svelte (Wash UI lucide-react 1.28.0).
 * Sync `__iconNode` only. No top-level await (WebKitGTK TDZ).
 * Note: lucide `filter-x.mjs` only re-exports default; import `funnel-x` for `__iconNode`.
 */
import { __iconNode as arrowUp } from 'lucide-react/dist/esm/icons/arrow-up.mjs';
import { __iconNode as check } from 'lucide-react/dist/esm/icons/check.mjs';
import { __iconNode as chevronDown } from 'lucide-react/dist/esm/icons/chevron-down.mjs';
import { __iconNode as chevronLeft } from 'lucide-react/dist/esm/icons/chevron-left.mjs';
import { __iconNode as chevronRight } from 'lucide-react/dist/esm/icons/chevron-right.mjs';
import { __iconNode as copy } from 'lucide-react/dist/esm/icons/copy.mjs';
import { __iconNode as download } from 'lucide-react/dist/esm/icons/download.mjs';
import { __iconNode as filterX } from 'lucide-react/dist/esm/icons/funnel-x.mjs';
import { __iconNode as folder } from 'lucide-react/dist/esm/icons/folder.mjs';
import { __iconNode as image } from 'lucide-react/dist/esm/icons/image.mjs';
import { __iconNode as minus } from 'lucide-react/dist/esm/icons/minus.mjs';
import { __iconNode as refreshCw } from 'lucide-react/dist/esm/icons/refresh-cw.mjs';
import { __iconNode as search } from 'lucide-react/dist/esm/icons/search.mjs';
import { __iconNode as square } from 'lucide-react/dist/esm/icons/square.mjs';
import { __iconNode as sun } from 'lucide-react/dist/esm/icons/sun.mjs';
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
	check,
	'arrow-up': arrowUp,
	'chevron-down': chevronDown,
	'chevron-left': chevronLeft,
	'chevron-right': chevronRight,
	'filter-x': filterX,
	folder,
	image,
	'refresh-cw': refreshCw,
	search
} as const satisfies Record<string, WashIconNode>;
