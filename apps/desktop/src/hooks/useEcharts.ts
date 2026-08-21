// An ECharts instance, loaded on demand.
//
// ECharts is the largest thing in the bundle and only one of three views needs it, so
// it is imported dynamically. The tree-shakeable entry never registers a renderer
// implicitly, so CanvasRenderer has to be asked for by name.
//
// Options are always applied with notMerge: the option is rebuilt whole from (graph,
// states, tokens, focus), so merging would leave removed nodes and edges on the canvas.
// We never call setTheme, which also keeps us clear of echarts#21200.

import { useEffect, useRef, useState } from 'react';

type EchartsCore = typeof import('echarts/core');
type EchartsInstance = ReturnType<EchartsCore['init']>;

let cached: EchartsCore | null = null;

export const loadEcharts = async (): Promise<EchartsCore> => {
	if (cached) return cached;
	const [core, charts, components, renderers, features] = await Promise.all([
		import('echarts/core'),
		import('echarts/charts'),
		import('echarts/components'),
		import('echarts/renderers'),
		import('echarts/features'),
	]);
	core.use([
		charts.GraphChart,
		components.TooltipComponent,
		features.LabelLayout,
		renderers.CanvasRenderer,
	]);
	cached = core;
	return core;
};

export const useEcharts = (option: Record<string, unknown> | null) => {
	const hostRef = useRef<HTMLDivElement>(null);
	const chartRef = useRef<EchartsInstance | null>(null);
	const [ready, setReady] = useState(false);

	useEffect(() => {
		let disposed = false;
		void (async () => {
			const echarts = await loadEcharts();
			if (disposed || !hostRef.current) return;
			chartRef.current = echarts.init(hostRef.current, undefined, {
				renderer: 'canvas',
				useDirtyRect: true,
			});
			setReady(true);
		})();
		return () => {
			disposed = true;
			chartRef.current?.dispose();
			chartRef.current = null;
		};
	}, []);

	useEffect(() => {
		if (ready && option) chartRef.current?.setOption(option, { notMerge: true });
	}, [ready, option]);

	useEffect(() => {
		const host = hostRef.current;
		if (!host || typeof ResizeObserver !== 'function') return;
		const observer = new ResizeObserver(() => chartRef.current?.resize());
		observer.observe(host);
		return () => observer.disconnect();
	}, []);

	return { hostRef, chart: chartRef, ready };
};
