type AreaConfig = {
  includeScopes: string[];
  excludeScopes: string[];
  excludeKeywords: string[];
};

const DEFAULT_AREA_CONFIGS: Record<string, AreaConfig> = {
  frontend: {
    includeScopes: ["frontend", "front-end", "ui", "web", "client", "ux", "design-system"],
    excludeScopes: ["backend", "api", "service", "infra", "ops", "db", "migration"],
    excludeKeywords: ["nuget", "csproj", "dotnet", "entityframework", "sql migration"],
  },
  backend: {
    includeScopes: ["backend", "api", "service", "db", "worker", "integration"],
    excludeScopes: ["frontend", "front-end", "ui", "web", "client", "ux"],
    excludeKeywords: ["storybook", "css", "tailwind", "nuxt", "react", "nextjs"],
  },
  infra: {
    includeScopes: ["infra", "ops", "devops", "pipeline", "ci", "cd", "k8s", "terraform"],
    excludeScopes: ["frontend", "front-end", "ui", "backend", "api"],
    excludeKeywords: ["feature flag", "ui test", "playwright"],
  },
};

function mergeAreaConfigs(customAreas?: Record<string, AreaConfig>): Record<string, AreaConfig> {
  if (!customAreas) {
    return { ...DEFAULT_AREA_CONFIGS };
  }

  const merged: Record<string, AreaConfig> = { ...DEFAULT_AREA_CONFIGS };
  for (const [name, config] of Object.entries(customAreas)) {
    const normalizedName = name.trim().toLowerCase();
    if (!normalizedName) {
      continue;
    }

    merged[normalizedName] = {
      includeScopes: config.includeScopes,
      excludeScopes: config.excludeScopes,
      excludeKeywords: config.excludeKeywords,
    };
  }

  return merged;
}

function listAreas(areaConfigs?: Record<string, AreaConfig>): string[] {
  const resolved = areaConfigs || DEFAULT_AREA_CONFIGS;
  return Object.keys(resolved).sort();
}

function getAreaConfig(name?: string, areaConfigs?: Record<string, AreaConfig>): AreaConfig | null {
  if (!name) {
    return null;
  }

  const resolved = areaConfigs || DEFAULT_AREA_CONFIGS;
  const normalized = name.trim().toLowerCase();
  return resolved[normalized] || null;
}

export { getAreaConfig, listAreas, mergeAreaConfigs };
export type { AreaConfig };
