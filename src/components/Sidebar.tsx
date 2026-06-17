interface SidebarProps {
  activeSection: string;
  projectCount: number;
  desktopReady: boolean;
  bridgeConnected: boolean;
  onNavigate: (section: string, label: string) => void;
}

const creationItems = [
  ["central", "⌂", "Dashboard"],
  ["sincronizacao", "≡", "Sincronização SRT"],
  ["producoes", "▦", "Produções"],
];

const operationItems = [
  ["sessoes", "◎", "Configurações"],
];

function NavItems({
  items,
  activeSection,
  projectCount,
  onNavigate,
}: SidebarProps & { items: string[][] }) {
  return items.map(([section, icon, label]) => (
    <button
      className={`nav-item ${activeSection === section ? "active" : ""}`}
      key={section}
      onClick={() => onNavigate(section, label)}
    >
      <span className="nav-icon">{icon}</span>
      {label}
      {section === "producoes" && <span className="nav-count">{projectCount}</span>}
    </button>
  ));
}

export function Sidebar(props: SidebarProps) {
  return (
    <aside className="sidebar">
      <a className="brand" href="#" aria-label="FlowContent Auto">
        <img src="/assets/logo.svg" alt="" />
        <span>
          <strong>FlowContent</strong>
          <small>AUTO</small>
        </span>
      </a>

      <nav className="primary-nav" aria-label="Navegação principal">
        <p className="nav-label">FLUXO</p>
        <NavItems {...props} items={creationItems} />
        <p className="nav-label">OPERAÇÃO</p>
        <NavItems {...props} items={operationItems} />
      </nav>

      <div className="connection-panel">
        <div className="connection-row">
          <span className={`pulse-dot ${props.desktopReady ? "" : "offline"}`} />
          <span>
            <strong>{props.bridgeConnected ? "Ponte Flow conectada" : props.desktopReady ? "Base desktop ativa" : "Prévia no navegador"}</strong>
            <small>{props.bridgeConnected ? "Extensão enviando heartbeat" : "Ponte Flow desconectada"}</small>
          </span>
        </div>
        <div className="guardrail">
          <span>Limites respeitados</span>
          <strong>SIM</strong>
        </div>
      </div>
    </aside>
  );
}
