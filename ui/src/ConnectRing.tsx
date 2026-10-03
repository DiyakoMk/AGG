type Props = {
  state: "idle" | "connecting" | "connected" | "disconnecting" | "error";
  disabled?: boolean;
  onClick: () => void;
};

function label(state: Props["state"]): string {
  switch (state) {
    case "connecting":
      return "Boosting";
    case "connected":
      return "Boosted";
    case "disconnecting":
      return "Stopping";
    case "error":
      return "Retry";
    default:
      return "Boost";
  }
}

export function ConnectRing({ state, disabled, onClick }: Props) {
  const live = state === "connected";
  const spin = state === "connecting" || state === "disconnecting";
  return (
    <button
      type="button"
      className={`ring ${live ? "is-on" : ""} ${spin ? "is-spin" : ""} ${state === "error" ? "is-err" : ""}`}
      disabled={disabled}
      onClick={onClick}
      aria-label={label(state)}
    >
      <svg viewBox="0 0 190 190" className="ring-svg" aria-hidden>
        <circle className="ring-track" cx="95" cy="95" r="88" />
        <circle className="ring-arc" cx="95" cy="95" r="88" />
      </svg>
      <span className="ring-label">{label(state)}</span>
    </button>
  );
}
