const YTDLP_LOGO = "/integrations/ytdlp/logo.svg";

interface IntegrationIconProps {
  integration: "ytdlp";
  size?: "sm" | "md" | "lg";
  className?: string;
}

const SIZE_CLASS = {
  sm: "integration-icon-sm",
  md: "integration-icon-md",
  lg: "integration-icon-lg",
} as const;

export default function IntegrationIcon({
  integration,
  size = "md",
  className = "",
}: IntegrationIconProps) {
  if (integration !== "ytdlp") return null;
  return (
    <span
      className={`integration-icon ${SIZE_CLASS[size]}${className ? ` ${className}` : ""}`}
      aria-hidden
    >
      <img src={YTDLP_LOGO} alt="" />
    </span>
  );
}

export { YTDLP_LOGO };
