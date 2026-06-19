exports.enrich = async ({ args, context }) => {
  return {
    $type: "EnrichedProfile",
    id: args[0],
    email: args[1],
    source: `javascript:${context ? context.actor : "unknown"}`
  };
};
