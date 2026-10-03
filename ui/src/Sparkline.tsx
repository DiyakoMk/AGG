type Props = {
  values: number[];
  width?: number;
  height?: number;
};

export function Sparkline({ values, width = 220, height = 48 }: Props) {
  if (values.length < 2) {
    return <svg width={width} height={height} aria-hidden />;
  }
  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = Math.max(1, max - min);
  const pad = 2;
  const innerH = height - pad * 2;
  const innerW = width - pad * 2;
  const pts = values.map((v, i) => {
    const x = pad + (i / (values.length - 1)) * innerW;
    const y = pad + innerH - ((v - min) / span) * innerH;
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  });
  return (
    <svg width={width} height={height} viewBox={`0 0 ${width} ${height}`} className="spark">
      <polyline fill="none" stroke="currentColor" strokeWidth="1.5" points={pts.join(" ")} />
    </svg>
  );
}
