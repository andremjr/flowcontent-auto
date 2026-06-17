import { useState } from "react";
import type { AssemblyAiStatus, FlowBridgeStatus, ProjectSummary } from "../types";

const stageLabels: Record<string, string> = {
  AWAITING_AUDIO: "Aguardando áudio",
  AWAITING_PROMPTS: "Aguardando prompts",
  READY_FOR_FLOW: "Pronto para o Flow",
  GENERATING_ASSETS: "Gerando assets",
};

function basename(path: string | null) {
  if (!path) return "Ainda não gerado";
  return path.split(/[\\/]/).pop() ?? path;
}

function ProjectStrip({
  projects,
  selected,
  onSelect,
}: {
  projects: ProjectSummary[];
  selected: ProjectSummary | null;
  onSelect: (id: string) => void;
}) {
  if (!projects.length) return null;
  return (
    <div className="project-strip">
      <span>PRODUÇÃO</span>
      {projects.map((project) => (
        <button
          className={project.localProjectId === selected?.localProjectId ? "active" : ""}
          key={project.localProjectId}
          onClick={() => onSelect(project.localProjectId)}
        >
          {project.title}
        </button>
      ))}
    </div>
  );
}

function EmptyModule({ title, description, action, onAction }: { title: string; description: string; action: string; onAction: () => void }) {
  return (
    <div className="module-empty">
      <span>▶</span>
      <h2>{title}</h2>
      <p>{description}</p>
      <button className="secondary-button" onClick={onAction}>{action}</button>
    </div>
  );
}

export function OverviewView({
  projects,
  bridgeConnected,
  onNavigate,
  onSelect,
  onCreate,
}: {
  projects: ProjectSummary[];
  bridgeConnected: boolean;
  onNavigate: (section: string) => void;
  onSelect: (id: string) => void;
  onCreate: () => void;
}) {
  const totalAssets = projects.reduce((sum, project) => sum + project.assetCount, 0);
  const ready = projects.filter((project) => project.stage === "READY_FOR_FLOW").length;
  return (
    <section className="module-page">
      <div className="module-hero">
        <div>
          <span className="section-kicker">MESA DE PRODUÇÃO</span>
          <h2>Da narração aos assets, sem perder a ordem.</h2>
          <p>Acompanhe o próximo passo de cada produção e entre direto no ponto que precisa de atenção.</p>
        </div>
        <button className="dispatch-button" onClick={onCreate}><span className="mini-play">▶</span> Nova produção</button>
      </div>
      <div className="metric-ribbon">
        <div><small>PRODUÇÕES</small><strong>{projects.length}</strong><span>pastas locais</span></div>
        <div><small>SLOTS NARRATIVOS</small><strong>{totalAssets}</strong><span>ordem preservada</span></div>
        <div><small>PRONTAS PARA FLOW</small><strong>{ready}</strong><span>prompts validados</span></div>
        <div><small>PONTE FLOW</small><strong className={bridgeConnected ? "bridge-on-value" : "hold-value"}>{bridgeConnected ? "ON" : "OFF"}</strong><span>{bridgeConnected ? "extensão conectada" : "extensão desconectada"}</span></div>
      </div>
      <div className="overview-grid">
        <section className="module-card span-two">
          <div className="module-card-head"><div><span className="section-kicker">PRÓXIMAS AÇÕES</span><h3>Fila de trabalho</h3></div></div>
          <div className="action-list">
            {projects.map((project) => (
              <button key={project.localProjectId} onClick={() => {
                onSelect(project.localProjectId);
                onNavigate(project.stage === "AWAITING_AUDIO" ? "sincronizacao" : "producoes");
              }}>
                <span className="action-index">▶</span>
                <span><strong>{project.title}</strong><small>{stageLabels[project.stage]}</small></span>
                <em>{project.assetCount || "—"} slots</em>
              </button>
            ))}
            {!projects.length && <p className="inline-empty">Crie uma produção para iniciar a fila de trabalho.</p>}
          </div>
        </section>
        <section className="module-card">
          <div className="module-card-head"><div><span className="section-kicker">GUARDRAILS</span><h3>Operação protegida</h3></div><span className="panel-state done">ATIVO</span></div>
          <div className="status-stack">
            <span><i className="ok" /><strong>Token local</strong><small>Sessão autenticada</small></span>
            <span><i className="ok" /><strong>Fila conservadora</strong><small>1 envio por vez</small></span>
            <span><i className={bridgeConnected ? "ok" : ""} /><strong>Extensão Flow</strong><small>{bridgeConnected ? "Heartbeat local ativo" : "Ainda não conectada"}</small></span>
          </div>
        </section>
      </div>
    </section>
  );
}

export function SynchronizationView({
  projects,
  selected,
  onSelect,
  busy,
  pendingAudioPath,
  onChooseAudio,
  onProcessAudio,
  onDownload,
  onNavigate,
}: {
  projects: ProjectSummary[];
  selected: ProjectSummary | null;
  onSelect: (id: string) => void;
  busy: boolean;
  pendingAudioPath: string | null;
  onChooseAudio: () => void;
  onProcessAudio: () => void;
  onDownload: (kind: "captions" | "assets") => void;
  onNavigate: (section: string) => void;
}) {
  const hasGeneratedSrts = Boolean(selected?.captionSrtPath || selected?.assetSrtPath);
  const currentAudioPath = selected?.audioPath ?? pendingAudioPath ?? null;

  return (
    <section className="module-page simple-page">
      <ProjectStrip projects={projects} selected={selected} onSelect={onSelect} />
      {!selected ? (
        <EmptyModule title="Crie uma produção primeiro" description="Dê um nome ao trabalho e depois envie o áudio." action="Criar produção" onAction={() => onNavigate("producoes")} />
      ) : (
        <>
          <div className="simple-step">
            <div className="step-number">01</div>
            <div className="step-copy">
              <span className="section-kicker">SINCRONIZAÇÃO SRT</span>
              <h2>{hasGeneratedSrts ? "SRTs prontos para baixar" : pendingAudioPath ? "Áudio pronto para processar" : "Envie a narração"}</h2>
              <p>{hasGeneratedSrts ? "Baixe os dois arquivos. Use o SRT de assets para criar os prompts em sua ferramenta preferida." : pendingAudioPath ? "O áudio foi selecionado. Revise o arquivo e depois gere os dois SRTs." : "Selecione o áudio da narração. Os arquivos SRT e o áudio copiado ficam salvos dentro da pasta local da produção."}</p>
              <p>Regra atual dos assets: pausa padrão de 100ms, duração mínima efetiva de 3s e duração máxima de 8s.</p>
              <p><strong>Áudio salvo no projeto:</strong> {basename(currentAudioPath)}</p>
            </div>
            <div className="session-actions">
              <button className="secondary-button" disabled={busy} onClick={onChooseAudio}>
                {currentAudioPath ? "Trocar áudio" : "Selecionar áudio"}
              </button>
              <button className="dispatch-button" disabled={busy || !currentAudioPath} onClick={onProcessAudio}>
                <span className="mini-play">▶</span>{busy ? "Gerando SRTs..." : "Gerar SRT"}
              </button>
            </div>
          </div>
          {hasGeneratedSrts && (
            <div className="download-list">
              <article>
                <span className="file-mark">CC</span>
                <div><strong>SRT de legendas</strong><small>{basename(selected.captionSrtPath)}</small><p>Para adicionar legendas ao vídeo final.</p></div>
                <button className="secondary-button" onClick={() => onDownload("captions")}>Baixar SRT</button>
              </article>
              <article>
                <span className="file-mark asset">A8</span>
                <div><strong>SRT de assets</strong><small>{basename(selected.assetSrtPath)}</small><p>{selected.assetCount} intervalos para gerar os prompts visuais, com mínimo de 3s e máximo de 8s por asset.</p></div>
                <button className="secondary-button" onClick={() => onDownload("assets")}>Baixar SRT</button>
              </article>
              <button className="quiet-button next-step-button" onClick={() => onNavigate("producoes")}>Já tenho os prompts, continuar para Produções →</button>
            </div>
          )}
        </>
      )}
    </section>
  );
}

export function SessionsView({
  bridge,
  assembly,
  busy,
  assemblyBusy,
  onOpenBrowser,
  onRefresh,
  onSaveAssemblyKeys,
  onClearAssemblyKeys,
}: {
  bridge: FlowBridgeStatus;
  assembly: AssemblyAiStatus;
  busy: boolean;
  assemblyBusy: boolean;
  onOpenBrowser: () => void;
  onRefresh: () => void;
  onSaveAssemblyKeys: (keys: string) => Promise<boolean>;
  onClearAssemblyKeys: () => void;
}) {
  const [assemblyKeys, setAssemblyKeys] = useState("");
  const saveAssemblyKeys = async () => {
    if (await onSaveAssemblyKeys(assemblyKeys)) setAssemblyKeys("");
  };
  return (
    <section className="module-page simple-page settings-page">
      <div className="step-copy">
        <span className="section-kicker">CONFIGURAÇÕES</span>
        <h2>Integrações</h2>
        <p>Essas configurações são feitas uma vez. Durante o trabalho, o navegador Flow permanece minimizado.</p>
      </div>
      <section className="module-card simple-setting-card">
        <div>
          <span className={`pulse-dot ${bridge.extensionConnected ? "" : "offline"}`} />
          <span><strong>Ponte com Google Flow</strong><small>{bridge.extensionConnected ? "Conectada e pronta para receber comandos" : bridge.extensionInstalled ? "Extensão instalada; reconecte a sessão" : "Instalação inicial necessária"}</small></span>
        </div>
        <div className="session-actions">
          <button className="quiet-button" onClick={onRefresh}>Atualizar</button>
          <button className="secondary-button" disabled={busy} onClick={onOpenBrowser}>{busy ? "Abrindo..." : bridge.extensionConnected ? "Reabrir sessão" : "Conectar Flow"}</button>
        </div>
      </section>
      <section className="module-card integration-card">
        <div className="module-card-head">
          <div><span className="section-kicker">TRANSCRICAO</span><h3>AssemblyAI</h3></div>
          <span className={`panel-state ${assembly.configured ? "done" : ""}`}>{assembly.configured ? "CONFIGURADA" : "CONFIGURAR"}</span>
        </div>
        <div className="integration-layout">
          <div className="integration-status">
            <span><small>CHAVES DISPONIVEIS</small><strong>{assembly.keyCount}</strong></span>
            <p>{assembly.configured ? `Configuradas: ${assembly.maskedKeys.join(", ")}` : "Adicione uma chave para liberar o processamento de audio."}</p>
            <p className="bridge-privacy-note">As chaves ficam no diretorio local do aplicativo, nao retornam para a interface e sao mascaradas nos diagnosticos.</p>
          </div>
          <div className="integration-form">
            <label>
              <span>Chave da AssemblyAI</span>
              <textarea
                rows={3}
                value={assemblyKeys}
                onChange={(event) => setAssemblyKeys(event.target.value)}
                placeholder="Cole uma chave por linha"
              />
            </label>
            <div className="integration-actions">
              {assembly.configured && <button className="quiet-button" disabled={assemblyBusy} onClick={onClearAssemblyKeys}>Remover chaves</button>}
              <button className="secondary-button" disabled={!assemblyKeys.trim() || assemblyBusy} onClick={saveAssemblyKeys}>
                {assemblyBusy ? "Salvando..." : "Salvar configuracao"}
              </button>
            </div>
          </div>
        </div>
      </section>
    </section>
  );
}
