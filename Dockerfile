# Build: docker build -t sslpinning-api .
# Run:  docker run -p 8080:8080 -e PORT=8080 sslpinning-api
FROM eclipse-temurin:17-jdk-alpine AS build
WORKDIR /app
COPY pom.xml .
COPY src ./src
RUN apk add --no-cache maven && mvn -q -DskipTests package

FROM eclipse-temurin:17-jre-alpine
WORKDIR /app
RUN addgroup -S app && adduser -S app -G app
COPY --from=build /app/target/*.jar app.jar
USER app
ENV PORT=8080
EXPOSE 8080
ENTRYPOINT ["sh", "-c", "exec java -jar app.jar"]
