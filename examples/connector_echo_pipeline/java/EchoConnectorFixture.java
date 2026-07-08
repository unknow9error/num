public final class EchoConnectorFixture {
  private EchoConnectorFixture() {}

  public static final class Echo implements NumConnectorSdk.EchoConnector {
    @Override
    public String reply(String message, NumConnectorSdk.NumConnectorContext context)
        throws NumConnectorSdk.NumConnectorException {
      if (context == null || context.tenant() == null || context.requestId() == null) {
        throw new NumConnectorSdk.NumConnectorException(
            "missing_context", "Num connector context is required", false);
      }
      return "java echo [" + context.requestId() + "]: " + message;
    }
  }

  public static final class Connectors implements NumConnectorSdk.NumConnectors {
    private final Echo echo = new Echo();

    @Override
    public NumConnectorSdk.EchoConnector echo() {
      return echo;
    }
  }
}
